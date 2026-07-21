import { describe, expect, test } from "bun:test";
import { existsSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";

const workflow = readFileSync(".github/workflows/release.yml", "utf8");
const signingConfig = JSON.parse(
  readFileSync("src-tauri/tauri.signing.conf.json", "utf8"),
);
const signingScript = readFileSync("scripts/sign-windows.ps1", "utf8");
const nsisTemplate = readFileSync("src-tauri/nsis/installer.nsi", "utf8");
const thirdPartyNotices = readFileSync("THIRD_PARTY_NOTICES.md", "utf8");

describe("bundled word-list notices", () => {
  test("carries the pinned SCOWL and VarCon copyright terms", () => {
    expect(thirdPartyNotices).toContain(
      "The collective work is Copyright 2000-2018 by Kevin Atkinson",
    );
    expect(thirdPartyNotices).toContain(
      "Copyright 2000-2016 by Kevin Atkinson",
    );
    expect(thirdPartyNotices).toContain("Copyright 2016 by Benjamin Titze");
    expect(thirdPartyNotices).toMatch(
      /Benjamin Titze makes no\s+representations about the suitability of\s+this array/,
    );
  });
});

function stepPosition(name: string): number {
  const position = workflow.indexOf(`- name: ${name}`);
  expect(position, `Missing workflow step: ${name}`).toBeGreaterThan(-1);
  return position;
}

describe("Windows release signing workflow", () => {
  test("limits signing to the protected environment and approved refs", () => {
    expect(workflow).toContain("group: release-windows");
    expect(workflow).toContain("environment: artifact-signing");
    expect(workflow).toContain("id-token: write");
    expect(workflow).toContain(
      "if: github.ref == 'refs/heads/main' || startsWith(github.ref, 'refs/tags/v')",
    );
    expect(workflow).toContain("runs-on: windows-2025");
    expect(workflow).toContain("persist-credentials: false");
  });

  test("uses passwordless Azure authentication", () => {
    expect(workflow).toContain(
      "uses: azure/login@532459ea530d8321f2fb9bb10d1e0bcf23869a43 # v3",
    );
    expect(workflow).toContain("client-id: ${{ vars.AZURE_CLIENT_ID }}");
    expect(workflow).toContain("tenant-id: ${{ vars.AZURE_TENANT_ID }}");
    expect(workflow).toContain(
      "subscription-id: ${{ vars.AZURE_SUBSCRIPTION_ID }}",
    );
    expect(workflow).not.toContain("AZURE_CLIENT_SECRET");
  });

  test("signs patched application copies during bundling and signs release outputs", () => {
    expect(workflow).not.toContain("tauri-apps/tauri-action");
    expect(workflow).toContain("bun run tauri build --no-bundle --ci");
    expect(workflow).toContain(
      "bun run tauri bundle --verbose --bundles nsis,msi --config src-tauri/tauri.signing.conf.json --ci",
    );

    const signingUses = workflow.match(
      /uses: azure\/artifact-signing-action@c7ab2a863ab5f9a846ddb8265964877ef296ee82 # v2/g,
    );
    expect(signingUses).toHaveLength(1);

    expect(workflow).toContain(
      "files: |\n            ${{ steps.signing-paths.outputs.app }}\n            ${{ steps.signing-paths.outputs.nsis }}\n            ${{ steps.signing-paths.outputs.msi }}",
    );

    const steps = [
      "Build application without bundling",
      "Authenticate to Azure",
      "Install Artifact Signing module",
      "Bundle installers",
      "Resolve installer paths",
      "Sign release outputs",
      "Verify Authenticode signatures",
      "Verify packaged application signatures",
      "Write SHA256SUMS",
      "Find or create draft release",
      "Upload signed installers to GitHub release",
      "Upload signed installers as CI artifact",
    ].map(stepPosition);

    for (let index = 1; index < steps.length; index += 1) {
      expect(steps[index]).toBeGreaterThan(steps[index - 1]);
    }
  });

  test("uses the configured Artifact Signing profile and timestamps", () => {
    expect(workflow).toContain(
      "endpoint: ${{ vars.ARTIFACT_SIGNING_ENDPOINT }}",
    );
    expect(workflow).toContain(
      "signing-account-name: ${{ vars.ARTIFACT_SIGNING_ACCOUNT_NAME }}",
    );
    expect(workflow).toContain(
      "certificate-profile-name: ${{ vars.ARTIFACT_SIGNING_CERTIFICATE_PROFILE_NAME }}",
    );
    expect(workflow).toContain(
      "timestamp-rfc3161: http://timestamp.acs.microsoft.com",
    );
    expect(workflow).toContain("timestamp-digest: SHA256");
    expect(workflow).toContain("file-digest: SHA256");
  });

  test("pins every action to a full commit", () => {
    const actions = [
      ...workflow.matchAll(/^\s*uses:\s+([^@\s]+)@([^\s#]+)(?:\s+#.*)?$/gm),
    ];

    expect(actions.length).toBeGreaterThan(0);
    for (const [, name, reference] of actions) {
      expect(reference, `${name} must use a full commit SHA`).toMatch(
        /^[0-9a-f]{40}$/,
      );
    }

    expect(workflow).toContain(
      "dtolnay/rust-toolchain@4cda84d5c5c54efe2404f9d843567869ab1699d4 # stable\n        with:\n          toolchain: stable",
    );
  });

  test("fails unless every expected output has a valid signature", () => {
    expect(workflow).toContain("Get-AuthenticodeSignature");
    expect(workflow).toContain("SignatureStatus]::Valid");
    expect(workflow).toContain("signtool.exe");
    expect(workflow).toContain("verify /pa /all /v");
    expect(workflow).toContain("if ($LASTEXITCODE -ne 0)");
    expect(workflow).toContain("TimeStamperCertificate");
    expect(workflow).toContain("CN=Joseph Amditis");
    expect(workflow).toContain("uninstall.exe");
  });

  test("publishes digests of the signed installers as a release asset", () => {
    // The website links to SHA256SUMS.txt at a fixed URL instead of carrying
    // hashes in its markup, so a missing asset leaves that link dead.
    expect(workflow).toContain(
      '$checksumPath = Join-Path $env:RUNNER_TEMP "SHA256SUMS.txt"',
    );
    expect(workflow).toContain(
      "$env:NSIS_PATH $env:MSI_PATH $env:CHECKSUM_PATH --clobber",
    );
    expect(workflow).toContain(
      "CHECKSUM_PATH: ${{ steps.checksums.outputs.path }}",
    );

    // Hashing a path that does not exist would otherwise publish a file
    // listing one installer and silently omit the other.
    expect(workflow).toContain("Cannot checksum a missing installer");

    // sha256sum -c wants lowercase hex, two spaces, a bare file name, LF, and
    // no BOM. Get-FileHash returns uppercase and Out-File writes CRLF+BOM.
    expect(workflow).toContain(
      "(Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()",
    );
    expect(workflow).toContain(
      '"$hash  $([System.IO.Path]::GetFileName($path))"',
    );
    expect(workflow).toContain("[System.Text.UTF8Encoding]::new($false)");
  });

  test("binds release assets to one commit and keeps reruns separate", () => {
    expect(workflow).toContain(
      'git rev-list -n 1 "refs/tags/$env:RELEASE_TAG"',
    );
    expect(workflow).toContain("targetCommitish");
    expect(workflow).toContain("must match workflow commit $env:GITHUB_SHA");
    expect(workflow).toContain("-${{ github.run_attempt }}");
  });

  test("verifies every downloaded package input before use", () => {
    expect(workflow).toContain(
      "SILERO_VAD_SHA256: a35ebf52fd3ce5f1469b2a36158dba761bc47b973ea3382b3186ca15b1f5af28",
    );
    expect(workflow).toContain("SILERO_VAD_BYTES: 1807522");
    expect(workflow).toContain(
      "VULKAN_RUNTIME_ARCHIVE_SHA256: 7d969f4d7b44e387667d3148f61559497c22d50cbe3d50adc9e5409afbce2df1",
    );
    expect(workflow).toContain("VULKAN_RUNTIME_ARCHIVE_BYTES: 15738272");
    expect(workflow).toContain(
      "cjpais/Handy/17d6c763413e3e29ec5cee76aa19ad01eccb73b2/src-tauri/resources/models/silero_vad_v4.onnx",
    );
    expect(workflow).toContain("Get-FileHash -LiteralPath");
    expect(workflow).toContain("Downloaded Silero VAD model hash mismatch");
    expect(workflow).toContain(
      "Downloaded Vulkan runtime archive hash mismatch",
    );
  });

  test("uses a CI-only Tauri signing command", () => {
    expect(signingConfig).toEqual({
      $schema: "https://schema.tauri.app/config/2",
      bundle: {
        windows: {
          signCommand: {
            cmd: "pwsh",
            args: [
              "-NoLogo",
              "-NoProfile",
              "-NonInteractive",
              "-File",
              "../scripts/sign-windows.ps1",
              "%1",
            ],
          },
        },
      },
    });

    expect(nsisTemplate).toContain(
      "!uninstfinalize '${UNINSTALLERSIGNCOMMAND} -TauriNsisUninstaller' = 0",
    );
  });

  test("keeps custom signer failures visible in the bundle log", () => {
    expect(workflow).toContain(
      "bun run tauri bundle --verbose --bundles nsis,msi --config src-tauri/tauri.signing.conf.json --ci",
    );
  });

  test("resolves the signing script from Tauri's project directory", () => {
    const args = signingConfig.bundle.windows.signCommand.args as string[];
    const fileArgument = args.indexOf("-File");
    expect(fileArgument).toBeGreaterThan(-1);

    const scriptPath = args[fileArgument + 1];
    const tauriDirectory = dirname(resolve("src-tauri/tauri.conf.json"));
    expect(existsSync(resolve(tauriDirectory, scriptPath))).toBe(true);
  });

  test("limits the Tauri signer to patched app copies and the NSIS uninstaller", () => {
    expect(signingScript).toContain("[switch] $TauriNsisUninstaller");
    expect(signingScript).toContain('-ieq "audiobud.exe"');
    expect(signingScript).toContain(
      "if (-not $isApplication -and -not $TauriNsisUninstaller)",
    );
    expect(signingScript).toContain(
      "Import-Module ArtifactSigning -RequiredVersion 0.1.8",
    );
    expect(signingScript).toContain("Invoke-ArtifactSigning");
    expect(signingScript).toContain("-ExcludeAzureCliCredential:$false");
    expect(signingScript).toContain("Get-AuthenticodeSignature");
  });

  test("allows NSIS to pass its temporary uninstaller name", () => {
    expect(nsisTemplate).toContain(
      "!uninstfinalize '${UNINSTALLERSIGNCOMMAND} -TauriNsisUninstaller' = 0",
    );
    expect(signingScript).not.toContain(
      "[System.IO.Path]::GetExtension($resolvedPath)",
    );
    expect(signingScript).not.toContain(
      "The NSIS uninstaller signing input must be an executable",
    );
  });

  test("binds credential exclusions as named boolean arguments", () => {
    const credentialExclusions = new Map<string, boolean>([
      ["ExcludeEnvironmentCredential", true],
      ["ExcludeWorkloadIdentityCredential", true],
      ["ExcludeManagedIdentityCredential", true],
      ["ExcludeSharedTokenCacheCredential", true],
      ["ExcludeVisualStudioCredential", true],
      ["ExcludeVisualStudioCodeCredential", true],
      ["ExcludeAzureCliCredential", false],
      ["ExcludeAzurePowerShellCredential", true],
      ["ExcludeAzureDeveloperCliCredential", true],
      ["ExcludeInteractiveBrowserCredential", true],
    ]);

    for (const [parameter, value] of credentialExclusions) {
      expect(signingScript).toContain(`-${parameter}:$${value}`);
    }

    expect(signingScript).not.toMatch(
      /-Exclude[A-Za-z]+Credential\s+\$(?:true|false)/,
    );
  });
});
