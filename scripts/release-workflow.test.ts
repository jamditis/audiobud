import { describe, expect, test } from "bun:test";
import { existsSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";

const workflow = readFileSync(".github/workflows/release.yml", "utf8");
const packageJson = JSON.parse(readFileSync("package.json", "utf8"));
const storeSubmission = readFileSync("STORE_SUBMISSION.md", "utf8");
const signingConfig = JSON.parse(
  readFileSync("src-tauri/tauri.signing.conf.json", "utf8"),
);
const microsoftStoreConfig = JSON.parse(
  readFileSync("src-tauri/tauri.microsoftstore.conf.json", "utf8"),
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

function stepBlock(name: string): string {
  const position = stepPosition(name);
  const nextStep = workflow.indexOf("\n      - name:", position + 1);
  return workflow.slice(position, nextStep === -1 ? undefined : nextStep);
}

function multilineInput(step: string, name: string): string[] {
  const values = new RegExp(
    `${name}: \\|\\r?\\n((?:[ \\t]{12}[^\\r\\n]+\\r?\\n?)+)`,
  ).exec(step)?.[1];
  expect(values, `Missing multiline input: ${name}`).toBeDefined();
  return values!
    .trim()
    .split(/\r?\n/)
    .map((value) => value.trim());
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

  test("takes the reproducible Rust build wins", () => {
    const reproducibleStep = stepBlock("Configure reproducible Rust build");
    expect(reproducibleStep).toContain("git show -s --format=%ct");
    expect(reproducibleStep).toContain("SOURCE_DATE_EPOCH=");
    expect(reproducibleStep).toContain(
      "--remap-path-prefix=$env:GITHUB_WORKSPACE=.",
    );
    expect(reproducibleStep).toContain(
      "--remap-path-prefix=$env:CARGO_TARGET_DIR=.target",
    );
    expect(reproducibleStep).toContain("RUSTFLAGS=");
    expect(stepPosition("Configure reproducible Rust build")).toBeLessThan(
      stepPosition("Build application without bundling"),
    );
  });

  test("signs patched application copies during bundling and signs release outputs", () => {
    expect(workflow).not.toContain("tauri-apps/tauri-action");
    expect(workflow).toContain(
      "bun run tauri build --no-bundle --ci -- --locked",
    );
    const githubBundle = stepBlock("Bundle GitHub installers");
    expect(githubBundle).toContain(
      "bun run tauri bundle --verbose --bundles nsis,msi `",
    );
    expect(githubBundle).toContain(
      "--config src-tauri/tauri.signing.conf.json --ci",
    );
    expect(githubBundle).not.toContain("tauri.updater.conf.json");
    expect(stepBlock("Bundle Store installers")).toContain(
      "bun run bundle:store",
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
      "Clear Store WebView2 installer cache",
      "Bundle Store installers",
      "Bundle GitHub installers",
      "Resolve installer paths",
      "Verify Store WebView2 offline installers",
      "Sign release outputs",
      "Verify Authenticode signatures",
      "Verify packaged application signatures",
      "Resolve SBOM path",
      "Generate release SBOM",
      "Write SHA256SUMS",
      "Attest release provenance",
      "Attest release SBOM",
      "Find or create draft release",
      "Upload release artifacts to GitHub release",
      "Upload release artifacts as CI artifact",
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
      '$checksumPath = Join-Path $env:CARGO_TARGET_DIR "release\\SHA256SUMS.txt"',
    );
    expect(workflow).toContain(
      "$env:NSIS_PATH $env:MSI_PATH $env:UPDATER_ARCHIVE",
    );
    expect(workflow).toContain(
      "$env:UPDATER_SIGNATURE $env:UPDATER_PUBLIC_KEY",
    );
    expect(workflow).toContain("$env:PORTABLE_PATH $env:SBOM_PATH");
    expect(workflow).toContain("$env:CHECKSUM_PATH --clobber");
    expect(workflow).toContain(
      "CHECKSUM_PATH: ${{ steps.checksums.outputs.path }}",
    );
    expect(workflow).toContain(
      "SBOM_PATH: ${{ steps.sbom-path.outputs.path }}",
    );

    const checksumStep = stepBlock("Write SHA256SUMS");
    expect(checksumStep).toContain(
      "$artifactPaths = @($env:NSIS_PATH, $env:MSI_PATH)",
    );
    expect(checksumStep).toContain("$artifactPaths += $env:PORTABLE_PATH");
    expect(checksumStep).toContain(
      "$lines = foreach ($path in $artifactPaths)",
    );
    expect(checksumStep).not.toContain("steps.sbom-path.outputs.path");

    // Hashing a path that does not exist would otherwise publish a file
    // listing one installer and silently omit the other.
    expect(workflow).toContain("Cannot checksum a missing release artifact");

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

  test("keeps CI artifact inputs under one filesystem tree", () => {
    // upload-artifact calculates one root for every path. Mixing installers
    // under CARGO_TARGET_DIR with a checksum under RUNNER_TEMP made the
    // Windows action fall back to GITHUB_WORKSPACE and reject the installers.
    const sbomPathStep = stepBlock("Resolve SBOM path");
    expect(sbomPathStep).toContain(
      '$directory = Join-Path $env:CARGO_TARGET_DIR "release"',
    );
    expect(sbomPathStep).not.toContain("$env:RUNNER_TEMP");

    const checksumStep = stepBlock("Write SHA256SUMS");
    expect(checksumStep).toContain(
      '$checksumPath = Join-Path $env:CARGO_TARGET_DIR "release\\SHA256SUMS.txt"',
    );
    expect(checksumStep).not.toContain("$env:RUNNER_TEMP");
  });

  test("generates a pinned release SBOM before checksums and attestations", () => {
    const sbomStep = stepBlock("Generate release SBOM");
    expect(sbomStep).toContain(
      "uses: anchore/sbom-action@e22c389904149dbc22b58101806040fa8d37a610 # v0.24.0",
    );
    expect(sbomStep).toContain(
      "path: ${{ steps.packaged-payload.outputs.path }}",
    );
    expect(sbomStep).not.toContain("path: .");
    expect(sbomStep).toContain("format: spdx-json");
    expect(sbomStep).toContain("syft-version: v1.49.0");
    expect(sbomStep).toContain(
      "output-file: ${{ steps.sbom-path.outputs.path }}",
    );
    expect(sbomStep).toContain("upload-artifact: false");
    expect(sbomStep).toContain("upload-release-assets: false");

    const payloadStep = stepBlock("Verify packaged application signatures");
    for (const dependencyFile of [
      "Cargo.lock",
      "Cargo.toml",
      "bun.lock",
      "package.json",
    ]) {
      expect(payloadStep).toContain(`\"${dependencyFile}\"`);
    }
    expect(payloadStep).toContain("dependency-manifests");

    const steps = [
      "Resolve SBOM path",
      "Generate release SBOM",
      "Write SHA256SUMS",
      "Attest release provenance",
      "Attest release SBOM",
    ].map(stepPosition);

    for (let index = 1; index < steps.length; index += 1) {
      expect(steps[index]).toBeGreaterThan(steps[index - 1]);
    }
  });

  test("attests provenance for every uploaded release artifact before publication", () => {
    expect(workflow).toContain("attestations: write");

    const provenanceStep = stepBlock("Attest release provenance");
    expect(provenanceStep).toContain(
      "uses: actions/attest@f7c74d28b9d84cb8768d0b8ca14a4bac6ef463e6 # v4.2.0",
    );
    expect(provenanceStep).not.toContain("\n        if:");

    const uploadedPaths = [
      "${{ steps.signing-paths.outputs.nsis }}",
      "${{ steps.signing-paths.outputs.msi }}",
      "${{ steps.updater-paths.outputs.archive }}",
      "${{ steps.updater-paths.outputs.signature }}",
      "${{ steps.updater-paths.outputs.public_key }}",
      "${{ steps.portable-webview-path.outputs.path }}",
      "${{ steps.sbom-path.outputs.path }}",
      "${{ steps.checksums.outputs.path }}",
    ];
    expect(multilineInput(provenanceStep, "subject-path")).toEqual(
      uploadedPaths,
    );
    expect(
      multilineInput(
        stepBlock("Upload release artifacts as CI artifact"),
        "path",
      ),
    ).toEqual(uploadedPaths);

    const steps = [
      "Generate release SBOM",
      "Write SHA256SUMS",
      "Attest release provenance",
      "Attest release SBOM",
      "Find or create draft release",
      "Upload release artifacts to GitHub release",
      "Upload release artifacts as CI artifact",
    ].map(stepPosition);

    for (let index = 1; index < steps.length; index += 1) {
      expect(steps[index]).toBeGreaterThan(steps[index - 1]);
    }
  });

  test("attests the SBOM as package metadata for the signed installers", () => {
    const sbomAttestationStep = stepBlock("Attest release SBOM");
    expect(sbomAttestationStep).toContain(
      "uses: actions/attest@f7c74d28b9d84cb8768d0b8ca14a4bac6ef463e6 # v4.2.0",
    );
    expect(sbomAttestationStep).not.toContain("\n        if:");
    expect(multilineInput(sbomAttestationStep, "subject-path")).toEqual([
      "${{ steps.signing-paths.outputs.nsis }}",
      "${{ steps.signing-paths.outputs.msi }}",
      "${{ steps.updater-paths.outputs.archive }}",
      "${{ steps.portable-webview-path.outputs.path }}",
    ]);
    expect(sbomAttestationStep).toContain(
      "sbom-path: ${{ steps.sbom-path.outputs.path }}",
    );
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
      "MODEL_ASSET_BASE_URL: https://github.com/jamditis/audiobud/releases/download/model-assets-v1",
    );
    expect(workflow).toContain(
      '"$env:MODEL_ASSET_BASE_URL/silero_vad_v4.onnx"',
    );
    expect(workflow).not.toContain("cjpais/Handy");
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

  test("keeps Microsoft Store packaging config opt-in", () => {
    const storeBundleScript = packageJson.scripts["bundle:store"];

    expect(storeBundleScript).toBe(
      "tauri bundle --verbose --bundles nsis,msi --config src-tauri/tauri.signing.conf.json --config src-tauri/tauri.microsoftstore.conf.json --ci",
    );
    expect(storeBundleScript.indexOf("tauri.signing.conf.json")).toBeLessThan(
      storeBundleScript.indexOf("tauri.microsoftstore.conf.json"),
    );
    expect(microsoftStoreConfig).toEqual({
      $schema: "https://schema.tauri.app/config/2",
      bundle: {
        createUpdaterArtifacts: false,
        windows: {
          webviewInstallMode: {
            type: "offlineInstaller",
          },
        },
      },
    });

    // The normal GitHub release continues to ship the current signed NSIS/MSI
    // artifacts. Store candidates opt into the offline WebView2 config when
    // we build a package for Partner Center.
    const storeBundleStep = stepBlock("Bundle Store installers");
    expect(storeBundleStep).toContain("bun run bundle:store");
    const githubBundleStep = stepBlock("Bundle GitHub installers");
    expect(githubBundleStep).toContain(
      "--config src-tauri/tauri.signing.conf.json --ci",
    );
    expect(githubBundleStep).not.toContain("tauri.updater.conf.json");
  });

  test("verifies the mutable Store WebView2 offline installers before upload", () => {
    const clearCacheStep = stepBlock("Clear Store WebView2 installer cache");
    expect(clearCacheStep).toContain(
      "if: ${{ env.STORE_CANDIDATE == 'true' }}",
    );
    expect(clearCacheStep).toContain('Join-Path $env:LOCALAPPDATA "tauri"');
    expect(clearCacheStep).toContain(
      'Where-Object { $_.Name -match "WebView2|MicrosoftEdge" }',
    );

    const webview2Step = stepBlock("Verify Store WebView2 offline installers");
    expect(webview2Step).toContain("if: ${{ env.STORE_CANDIDATE == 'true' }}");
    expect(webview2Step).toContain(
      "MSI_PATH: ${{ steps.signing-paths.outputs.msi }}",
    );
    expect(webview2Step).toContain(
      "function Assert-MicrosoftWebView2Signature",
    );
    expect(webview2Step).toContain(
      'Where-Object { $_.Name -match "WebView2|MicrosoftEdge" -and $_.Extension -eq ".exe" }',
    );
    expect(webview2Step).toContain(
      "Expected at least one Store WebView2 offline installer in the Tauri cache",
    );
    expect(webview2Step).toContain(
      "Expected at least one MSI-embedded WebView2 offline installer",
    );
    expect(webview2Step).toContain("Get-AuthenticodeSignature");
    expect(webview2Step).toContain("CN=Microsoft Corporation");
    expect(webview2Step).toContain("verify /pa /v");
    expect(webview2Step).not.toContain("verify /pa /all /v $Path");
    expect(webview2Step).toContain("Unexpected WebView2 installer signer");
    expect(webview2Step).toContain('Filter "dark.exe"');
    expect(webview2Step).toContain(
      "dark.exe failed to extract MSI embedded binaries",
    );
    expect(webview2Step).toContain(
      '$webView2PayloadDirectory = Join-Path $env:CARGO_TARGET_DIR "release\\webview2-payload"',
    );
    expect(webview2Step).toContain(
      "Copy-Item -LiteralPath $embeddedInstaller.FullName",
    );
    expect(webview2Step).toContain(
      '"path=$webView2PayloadDirectory" |\n            Out-File -FilePath $env:GITHUB_OUTPUT -Append -Encoding utf8',
    );
  });

  test("keeps Store candidate artifacts out of GitHub releases", () => {
    expect(workflow).toContain("store_candidate:");
    expect(workflow).toContain(
      "Build a Microsoft Store candidate CI artifact with offline WebView2 packaging",
    );
    expect(workflow).toContain(
      "STORE_CANDIDATE: ${{ github.event_name == 'workflow_dispatch' && inputs.store_candidate }}",
    );
    expect(stepBlock("Validate release mode")).toContain(
      "Store candidate artifacts cannot be published as a GitHub release",
    );
    expect(stepBlock("Validate release mode")).toContain(
      "Store candidate artifacts must be built from main, not tag refs",
    );
    expect(stepBlock("Find or create draft release")).toContain(
      "env.STORE_CANDIDATE != 'true'",
    );
    expect(stepBlock("Upload release artifacts to GitHub release")).toContain(
      "env.STORE_CANDIDATE != 'true'",
    );
    expect(stepBlock("Resolve package flavor")).toContain(
      '"artifact_suffix=-store"',
    );
    expect(workflow).toContain(
      "name: audiobud-windows-x86_64${{ steps.package-flavor.outputs.artifact_suffix }}-v${{ steps.meta.outputs.version }}-${{ github.run_attempt }}",
    );
  });

  test("documents the Microsoft Store package checkpoint", () => {
    expect(storeSubmission).toContain("Product type: `EXE or MSI app`.");
    expect(storeSubmission).toContain("Availability: `United States` only.");
    expect(storeSubmission).toContain("Pricing: `Free: no payment necessary`.");
    expect(storeSubmission).toContain("Category: `Productivity`.");
    expect(storeSubmission).toContain(
      "Privacy policy URL: `https://audiobud.amditis.tech/privacy.html`.",
    );
    expect(storeSubmission).toContain("App type: `MSI`.");
    expect(storeSubmission).toContain("Architecture: `x64`.");
    expect(storeSubmission).toContain(
      "Status: approved and available in the Microsoft Store on August 2, 2026.",
    );
    expect(storeSubmission).toContain("Submitted package ID: `55846694`.");
    expect(storeSubmission).toContain(
      "Submitted package URL:\n  `https://share.amditis.tech/audiobud/downloads/0.4.1/AudioBud_0.4.1_x64_en-US.msi`.",
    );
    expect(storeSubmission).toContain(
      "Submitted MSI SHA-256:\n  `9ee9d66d75abf7522bd5986c0c3bb0bb629d6274c80dafe35826aea29ccca3c3`.",
    );
    expect(storeSubmission).toContain(
      "Installer parameters: `/qn /norestart`.",
    );
    expect(storeSubmission).toContain("Language: `English (United States)`.");
    expect(storeSubmission).toContain("Do not use a `/latest` URL.");
    expect(storeSubmission).toContain("bun run bundle:store");
    expect(storeSubmission).toContain("src-tauri/tauri.signing.conf.json");
    expect(storeSubmission).toContain(
      "src-tauri/tauri.microsoftstore.conf.json",
    );
  });

  test("keeps new Store installs on AudioBud's signed NSIS update channel", () => {
    expect(storeSubmission).toContain(
      "Store listing: `https://apps.microsoft.com/detail/xpff8hfmd98gnd`.",
    );
    expect(storeSubmission).toContain("Replacement app type: `EXE`.");
    expect(storeSubmission).toContain(
      "Replacement installer parameters: `/S`.",
    );
    expect(storeSubmission).toContain(
      "Use the generated NSIS executable for the replacement Store submission.",
    );
    expect(storeSubmission).toContain(
      "Replace the Store package with the signed Store-candidate NSIS build",
    );
    expect(storeSubmission).toContain(
      "Do not substitute the normal GitHub release asset",
    );
    expect(storeSubmission).toContain(
      "The published 0.4.1 MSI cannot receive AudioBud's signed in-app updates",
    );
    expect(storeSubmission).toMatch(
      /After that one-time\s+transition, Store users receive signed updates through AudioBud's update feed\./,
    );

    const webview2Step = stepBlock("Verify Store WebView2 offline installers");
    expect(webview2Step).toContain(
      "NSIS_PATH: ${{ steps.signing-paths.outputs.nsis }}",
    );
    expect(webview2Step).toContain('Get-Command "7z.exe"');
    expect(webview2Step).toContain(
      'Join-Path $env:ProgramFiles "7-Zip\\7z.exe"',
    );
    expect(webview2Step).toContain("7z.exe was not found");
    expect(webview2Step).toContain(
      "Expected at least one NSIS-embedded WebView2 offline installer",
    );

    const packagedVerificationStep = stepBlock(
      "Verify packaged application signatures",
    );
    expect(packagedVerificationStep).toContain(
      "APP_VERSION: ${{ steps.meta.outputs.version }}",
    );
    expect(packagedVerificationStep).toContain(
      'if ($env:STORE_CANDIDATE -eq "true")',
    );
    expect(packagedVerificationStep).toContain(
      "Store candidate $env:APP_VERSION is behind live update feed",
    );
    expect(packagedVerificationStep).toContain('"--install-update"');
    expect(packagedVerificationStep).toContain(
      "https://github.com/jamditis/audiobud/releases/download/update-feed/latest.json",
    );
    expect(packagedVerificationStep).toContain(
      "Store NSIS signed-update probe failed",
    );
  });

  test("documents the Store silent-install candidate behavior", () => {
    expect(workflow).toContain(
      'Start-Process -FilePath "$env:SystemRoot\\System32\\msiexec.exe"',
    );
    expect(workflow).toContain('"/i"');
    expect(workflow).toContain('"/qn"');
    expect(workflow).toContain('"/norestart"');
    expect(workflow).toContain("MSI silent install failed");
    expect(workflow).toContain('"/x"');
    expect(workflow).toContain("MSI silent uninstall failed");
    expect(workflow).toContain('-ArgumentList @("/S", "/D=$nsisDirectory")');
    expect(workflow).toContain('-ArgumentList "/S" -Wait -PassThru');
    expect(nsisTemplate).toContain("${OrIf} ${Silent}");
    expect(nsisTemplate).toContain("CreateOrUpdateStartMenuShortcut");
    expect(nsisTemplate).toContain(
      'WriteRegStr SHCTX "${UNINSTKEY}" "UninstallString" "$\\"$INSTDIR\\uninstall.exe$\\""',
    );
  });

  test("verifies all packaged PE files for Store compatibility", () => {
    const packagedVerificationStep = stepBlock(
      "Verify packaged application signatures",
    );

    expect(packagedVerificationStep).toContain(
      "function Assert-PackagedPeSignatures",
    );
    expect(packagedVerificationStep).toContain(
      'Where-Object { $_.Extension -in @(".exe", ".dll") }',
    );
    expect(packagedVerificationStep).toContain("Invalid packaged PE signature");
    expect(packagedVerificationStep).toContain(
      "signtool verification failed for packaged PE file",
    );
    expect(packagedVerificationStep).toContain(
      "Assert-PackagedPeSignatures -Root $msiDirectory",
    );
    expect(packagedVerificationStep).toContain(
      "Assert-PackagedPeSignatures -Root $nsisDirectory",
    );
    expect(packagedVerificationStep).toContain(
      '$sbomPayloadDirectory = Join-Path $env:CARGO_TARGET_DIR "release\\sbom-payload"',
    );
    expect(packagedVerificationStep).toContain(
      "Copy-Item -LiteralPath $msiDirectory",
    );
    expect(packagedVerificationStep).toContain(
      "Copy-Item -LiteralPath $nsisDirectory",
    );
    expect(packagedVerificationStep).toContain(
      "Copy-Item -LiteralPath $env:WEBVIEW2_PAYLOAD_PATH",
    );
    expect(packagedVerificationStep).toContain(
      '"path=$sbomPayloadDirectory" |\n            Out-File -FilePath $env:GITHUB_OUTPUT -Append -Encoding utf8',
    );
  });

  test("keeps custom signer failures visible in the bundle log", () => {
    expect(stepBlock("Bundle GitHub installers")).toContain(
      "bun run tauri bundle --verbose --bundles nsis,msi `",
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

  test("limits the Tauri signer to patched app copies, final installers, and the NSIS uninstaller", () => {
    expect(signingScript).toContain("[switch] $TauriNsisUninstaller");
    expect(signingScript).toContain('-ieq "audiobud.exe"');
    expect(signingScript).toContain("$isFinalNsis");
    expect(signingScript).toContain("$isFinalMsi");
    expect(signingScript).toContain("^AudioBud_");
    expect(signingScript).toContain("_x64-setup\\.exe$");
    expect(signingScript).toContain("_x64_en-US\\.msi$");
    expect(signingScript).toContain("if (-not $isApprovedInput)");
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
