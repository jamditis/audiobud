import { describe, expect, test } from "bun:test";
import { existsSync, readFileSync } from "node:fs";

const workflow = readFileSync(".github/workflows/release.yml", "utf8");
const ciWorkflow = readFileSync(".github/workflows/ci.yml", "utf8");
const config = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8"));
const storeConfig = JSON.parse(
  readFileSync("src-tauri/tauri.microsoftstore.conf.json", "utf8"),
);

function stepBlock(name: string): string {
  const position = workflow.indexOf(`- name: ${name}`);
  expect(position, `Missing workflow step: ${name}`).toBeGreaterThan(-1);
  const next = workflow.indexOf("\n      - name:", position + 1);
  return workflow.slice(position, next === -1 ? undefined : next);
}

function stepPosition(name: string): number {
  const position = workflow.indexOf(`- name: ${name}`);
  expect(position, `Missing workflow step: ${name}`).toBeGreaterThan(-1);
  return position;
}

describe("signed updater release artifacts", () => {
  test("serializes every release mutation for one source ref", () => {
    expect(workflow).toMatch(
      /\nconcurrency:\n  group: release-\$\{\{ github\.ref \}\}\n  cancel-in-progress: false\n\njobs:/,
    );
  });

  test("creates and signs updater artifacts after unrelated bundle hooks", () => {
    expect(config.bundle.createUpdaterArtifacts).toBe(false);
    expect(storeConfig.bundle.createUpdaterArtifacts).toBe(false);
    expect(existsSync("src-tauri/tauri.updater.conf.json")).toBe(false);

    const bundle = stepBlock("Bundle GitHub installers");
    expect(bundle).not.toContain("TAURI_SIGNING_PRIVATE_KEY");
    expect(bundle).not.toContain("createUpdaterArtifacts");

    const archive = stepBlock("Create updater archive");
    expect(archive).toContain("Compress-Archive");
    expect(archive).toContain("-CompressionLevel NoCompression");
    expect(archive).not.toContain("TAURI_SIGNING_PRIVATE_KEY");

    const signer = stepBlock("Sign updater archive");
    expect(signer).toContain("bun run tauri signer sign");
    expect(signer).not.toContain("tauri bundle");
    expect(signer).toContain(
      "TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}",
    );
    expect(signer).toContain(
      "TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}",
    );
    expect(
      workflow.match(/secrets\.TAURI_SIGNING_PRIVATE_KEY\s*}}/g),
    ).toHaveLength(1);
    expect(
      workflow.match(/secrets\.TAURI_SIGNING_PRIVATE_KEY_PASSWORD\s*}}/g),
    ).toHaveLength(1);
    expect(workflow.indexOf("- name: Bundle GitHub installers")).toBeLessThan(
      workflow.indexOf("- name: Sign updater archive"),
    );
    const jobEnvironment = workflow.slice(0, workflow.indexOf("    steps:"));
    expect(jobEnvironment).not.toContain("TAURI_SIGNING_PRIVATE_KEY");
    expect(stepBlock("Bundle Store installers")).not.toContain(
      "TAURI_SIGNING_PRIVATE_KEY",
    );
  });

  test("resolves and validates the signed updater archive", () => {
    const paths = stepBlock("Resolve updater artifact paths");
    expect(paths).toContain("AudioBud_$($env:APP_VERSION)_x64-setup.nsis.zip");
    expect(paths).toContain('"archive=$archive"');
    expect(paths).toContain('"signature=$signature"');
    expect(paths).toContain("Get-AuthenticodeSignature");
    expect(paths).toContain("CN=Joseph Amditis");
    expect(paths).toContain("Expand-Archive");
  });

  test("verifies the updater signature against the release signing key", () => {
    const verifierPath = "scripts/verify-updater-signature/src/main.rs";
    expect(existsSync(verifierPath)).toBe(true);
    const verifier = readFileSync(verifierPath, "utf8");
    expect(verifier).toContain("minisign_verify");
    expect(verifier).toContain("verify_stream");
    expect(verifier).toContain("finalize()");

    const paths = stepBlock("Resolve updater artifact paths");
    expect(paths).toContain(
      "TAURI_SIGNING_PUBLIC_KEY: ${{ vars.TAURI_SIGNING_PUBLIC_KEY }}",
    );
    expect(paths).toContain("updater-signing-public-key.pub");
    expect(paths).toContain("scripts/validate-updater-key-transition.ts");
    expect(paths).toContain("Updater signing key transition validation failed");
    expect(paths).toContain("[Convert]::FromBase64String");
    expect(paths).toContain("scripts/verify-updater-signature/Cargo.toml");
    expect(paths).toContain("cargo run --locked --release");
  });

  test("publishes and attests updater files without publishing latest.json", () => {
    for (const stepName of [
      "Attest release provenance",
      "Upload release artifacts to GitHub release",
      "Upload release artifacts as CI artifact",
    ]) {
      const step = stepBlock(stepName);
      expect(step).toContain("steps.updater-paths.outputs.archive");
      expect(step).toContain("steps.updater-paths.outputs.signature");
      expect(step).toContain("steps.updater-paths.outputs.public_key");
      expect(step).not.toContain("latest.json");
    }
    expect(workflow).not.toContain("- name: Generate latest.json");
    expect(workflow).not.toContain("- name: Publish latest.json");
  });

  test("applies the exact private draft updater before publication", () => {
    expect(workflow).toContain("verify-updater-candidate:");
    expect(workflow).toMatch(
      /verify-updater-candidate:[\s\S]*?needs: build-windows[\s\S]*?runs-on: windows-2025/,
    );
    const jobStart = workflow.indexOf("  verify-updater-candidate:");
    const jobEnd = workflow.indexOf("\n  build-macos:", jobStart);
    const verificationJob = workflow.slice(jobStart, jobEnd);
    expect(verificationJob).toContain("always() &&");
    expect(verificationJob).toContain("inputs.verify_existing_draft");
    expect(verificationJob).not.toContain("environment: artifact-signing");
    expect(verificationJob).toContain("contents: write");
    expect(verificationJob).toContain(
      "artifact-name: ${{ steps.updater-meta.outputs.evidence_artifact }}",
    );
    expect(stepPosition("Resolve private updater draft")).toBeLessThan(
      stepPosition("Download private updater draft files"),
    );
    expect(stepPosition("Download private updater draft files")).toBeLessThan(
      stepPosition("Verify private updater provenance"),
    );
    expect(stepPosition("Verify private updater provenance")).toBeLessThan(
      stepPosition("Verify private updater signature"),
    );
    expect(stepPosition("Verify private updater signature")).toBeLessThan(
      stepPosition("Apply private updater and preserve user data"),
    );
    expect(
      stepPosition("Apply private updater and preserve user data"),
    ).toBeLessThan(
      stepPosition("Upload private updater verification evidence"),
    );

    const verification = stepBlock(
      "Apply private updater and preserve user data",
    );
    expect(verification).toContain("scripts/verify-updater-candidate.ps1");
    expect(verification).toContain("steps.updater-meta.outputs.prior_tag");
    expect(verification).toContain("steps.updater-meta.outputs.prior_version");
    expect(verification).not.toContain("$LASTEXITCODE");
    expect(verification).not.toContain("dangerousInsecureTransportProtocol");

    const metadata = stepBlock("Resolve private updater draft");
    expect(metadata).toContain("gh release view");
    expect(metadata).toContain("target_commit=$tagCommit");
    expect(metadata).not.toContain("releases/tags/$env:RELEASE_TAG");
    expect(metadata).toContain("releases?per_page=100");
    expect(metadata).toContain("--paginate --slurp");
    expect(metadata).toContain("scripts/select-latest-stable-app-release.mjs");
    expect(metadata).not.toContain("-match '^v");
    expect(metadata).not.toContain("-notmatch '^v");
    expect(metadata).not.toContain("releases/latest");
    expect(metadata).toContain("prior_tag=$priorTag");
    expect(metadata).toContain("prior_version=$priorVersion");
    const priorInstaller = stepBlock("Download latest prior stable installer");
    expect(priorInstaller).toContain("steps.updater-meta.outputs.prior_tag");
    expect(priorInstaller).toContain(
      "steps.updater-meta.outputs.prior_version",
    );
  });

  test("retains private proof that settings and models survive", () => {
    const verifierPath = "scripts/verify-updater-candidate.ps1";
    const serverPath = "scripts/serve-updater-candidate.mjs";
    expect(existsSync(verifierPath)).toBe(true);
    expect(existsSync(serverPath)).toBe(true);

    const verifier = existsSync(verifierPath)
      ? readFileSync(verifierPath, "utf8")
      : "";
    expect(verifier).toContain("settings_store.json");
    expect(verifier).toContain("audio_feedback");
    expect(verifier).toContain("models");
    expect(verifier).toContain("Get-FileHash");
    expect(verifier).toContain("https://localhost");
    expect(verifier).toContain("Import-Certificate");
    expect(verifier).toContain("Cert:\\CurrentUser\\Root");
    expect(verifier).toContain("finally");
    expect(verifier).toContain("--install-update-endpoint");
    expect(verifier).toContain("model_sha256_before");
    expect(verifier).toContain("model_sha256_after");
    expect(verifier).toContain("moonshine-tiny-streaming-en.tar.gz");
    expect(verifier).toContain(
      "465addcfca9e86117415677dfdc98b21edc53537210333a3ecdb58509a80abaf",
    );
    expect(verifier).toContain("Get-DirectoryInventorySha256");
    expect(verifier).not.toContain("release-preservation-sentinel.bin");
    expect(verifier).toContain("Wait-ForUpdaterQuiescence");
    expect(verifier).not.toContain("Wait-ForUpdaterRelaunch");
    expect(verifier).toContain('"AudioBud_${TargetVersion}_x64-setup"');
    expect(verifier).toContain("settings_value_before");
    expect(verifier).toContain("settings_value_after");
    expect(verifier).toContain("workflow_run_attempt");
    expect(verifier).not.toContain("[string]$PriorTag = 'v0.5.0'");
    expect(verifier).not.toContain("[string]$PriorVersion = '0.5.0'");
    expect(verifier).toContain("TimeStamperCertificate");
    expect(verifier).toContain("uninstall.exe");
    expect(verifier).not.toContain("    -Wait `");
    expect(verifier).toContain("$installProcess.WaitForExit(300000)");
    expect(verifier).toContain("$uninstallProcess.WaitForExit(300000)");
    expect(verifier.match(/\.Kill\(\$true\)/g) ?? []).toHaveLength(3);
    expect(verifier).toContain("updater-verification-stage.log");
    expect(verifier).toContain("updater-verification-error.log");
    expect(verifier).toContain("updater-prepublication-failure.json");
    expect(verifier).toContain("failure_stage");
    expect(verifier).toContain("ConnectionTimeoutSeconds");
    expect(verifier).toContain("OperationTimeoutSeconds");
    expect(verifier).toContain(
      "[ValidateSet('github-actions', 'local-windows')]",
    );
    expect(verifier).toContain("Local Windows execution requires ExecutionId");
    expect(verifier).toContain(
      "GitHub Actions execution requires GITHUB_ACTIONS=true",
    );
    expect(verifier).toContain("Evidence directory must not already exist");
    expect(verifier).toContain("execution_environment");
    expect(verifier).toContain("verifier_script_sha256");
    expect(verifier).toContain("host_architecture");
    expect(verifier).toContain("Get-AudioBudUninstallRegistryPaths");
    expect(verifier).toContain("Get-OptionalRegistryStringValue");
    const optionalValueReader = verifier.slice(
      verifier.indexOf("function Get-OptionalRegistryStringValue"),
      verifier.indexOf("function Normalize-AudioBudInstallDirectory"),
    );
    expect(optionalValueReader).toContain("if ($null -eq $Values)");
    expect(optionalValueReader.indexOf("if ($null -eq $Values)")).toBeLessThan(
      optionalValueReader.indexOf("$Values.PSObject.Properties[$Name]"),
    );
    const registryScanStart = verifier.indexOf(
      "function Get-AudioBudUninstallRegistryPaths",
    );
    const registrationAssertionStart = verifier.indexOf(
      "function Assert-AudioBudUninstallRegistration",
    );
    const updaterDirectoryFunctionsStart = verifier.indexOf(
      "function Get-AudioBudUpdaterDirectories",
    );
    expect(registryScanStart).toBeGreaterThan(-1);
    expect(registrationAssertionStart).toBeGreaterThan(registryScanStart);
    expect(updaterDirectoryFunctionsStart).toBeGreaterThan(
      registrationAssertionStart,
    );
    const registryScan = verifier.slice(
      registryScanStart,
      registrationAssertionStart,
    );
    const registrationAssertion = verifier.slice(
      registrationAssertionStart,
      updaterDirectoryFunctionsStart,
    );
    expect(
      registryScan.match(/Get-OptionalRegistryStringValue/g) ?? [],
    ).toHaveLength(3);
    for (const registryValueName of [
      "DisplayName",
      "InstallLocation",
      "UninstallString",
    ]) {
      expect(registryScan).toContain(`-Name '${registryValueName}'`);
    }
    expect(registryScan).toContain("$displayName -ceq 'AudioBud' -or");
    expect(registrationAssertion).toContain(
      "$displayName -ceq 'AudioBud' -and",
    );
    expect(registrationAssertion).toContain(
      "Normalize-AudioBudInstallDirectory -Path $installLocation",
    );
    expect(verifier).toContain(".Trim('\"')");
    expect(verifier).not.toContain("[string]$values.DisplayName");
    expect(verifier).not.toContain("[string]$values.InstallLocation");
    expect(verifier).not.toContain("[string]$values.UninstallString");
    expect(verifier).toContain("installedRegistryPaths");
    expect(verifier).toContain("targetRegistryPaths");
    expect(verifier).toContain("remainingRegistryPaths");
    expect(verifier).toContain(
      "Updated installation created no AudioBud uninstall registration",
    );
    expect(verifier).toContain("function Assert-AudioBudUninstallRegistration");
    expect(verifier).toContain("-Name 'DisplayVersion'");
    const priorRegistrationCheck = verifier.slice(
      verifier.indexOf("$installedRegistryPaths"),
      verifier.indexOf("$priorRestart"),
    );
    expect(priorRegistrationCheck).toContain(
      "Assert-AudioBudUninstallRegistration",
    );
    expect(priorRegistrationCheck).toContain("-ExpectedVersion $PriorVersion");
    expect(priorRegistrationCheck).not.toContain("$TargetVersion");
    const targetRegistrationCheck = verifier.slice(
      verifier.indexOf("$targetRegistryPaths"),
      verifier.indexOf("$uninstaller = Join-Path"),
    );
    expect(targetRegistrationCheck).toContain(
      "Assert-AudioBudUninstallRegistration",
    );
    expect(targetRegistrationCheck).toContain(
      "-ExpectedVersion $TargetVersion",
    );
    expect(targetRegistrationCheck).not.toContain("$PriorVersion");
    expect(verifier).toContain(
      "Updated uninstall left AudioBud registration keys",
    );
    const targetRegistryPosition = verifier.indexOf("targetRegistryPaths");
    const updatedUninstallerPosition = verifier.indexOf(
      "$uninstaller = Join-Path $installDirectory 'uninstall.exe'",
      targetRegistryPosition,
    );
    expect(targetRegistryPosition).toBeLessThan(updatedUninstallerPosition);
    expect(verifier.indexOf("remainingRegistryPaths")).toBeGreaterThan(
      updatedUninstallerPosition,
    );
    expect(verifier).toContain("Updated uninstall left the install directory");
    expect(verifier).toContain("Get-NewAudioBudUpdaterDirectories");
    expect(verifier).toContain("updaterDirectoriesBefore");
    expect(verifier).toContain("updaterExtractionDirectories");
    expect(verifier).toContain("Updater extraction directory cleanup failed");
    const updaterCleanupStart = verifier.indexOf(
      "foreach ($updaterDirectory in @($updaterExtractionDirectories))",
    );
    const certificateCleanupStart = verifier.indexOf(
      "foreach ($trustedCertificate in @($rootCertificate))",
    );
    expect(updaterCleanupStart).toBeGreaterThan(-1);
    expect(certificateCleanupStart).toBeGreaterThan(updaterCleanupStart);
    const updaterCleanup = verifier.slice(
      updaterCleanupStart,
      certificateCleanupStart,
    );
    expect(updaterCleanup).toContain(
      "$updaterDirectoryParent -ine $tempRootFullPath",
    );
    expect(updaterCleanup).toContain("$updaterDirectoryName.StartsWith(");
    expect(updaterCleanup).toContain('"AudioBud-${TargetVersion}-updater-"');
    expect(updaterCleanup).toContain(
      "Refusing to remove unexpected updater directory",
    );
    expect(updaterCleanup).toContain("[System.StringComparison]::Ordinal");
    expect(updaterCleanup).toContain(
      "Remove-Item -LiteralPath $updaterDirectory -Recurse",
    );
    const parentGuard = updaterCleanup.indexOf(
      "$updaterDirectoryParent -ine $tempRootFullPath",
    );
    const prefixGuard = updaterCleanup.indexOf(
      "$updaterDirectoryName.StartsWith(",
    );
    const refusal = updaterCleanup.indexOf(
      "Refusing to remove unexpected updater directory",
    );
    const recursiveDelete = updaterCleanup.indexOf(
      "Remove-Item -LiteralPath $updaterDirectory -Recurse",
    );
    expect(parentGuard).toBeGreaterThan(-1);
    expect(prefixGuard).toBeGreaterThan(-1);
    expect(refusal).toBeGreaterThan(parentGuard);
    expect(refusal).toBeGreaterThan(prefixGuard);
    expect(recursiveDelete).toBeGreaterThan(refusal);
    expect(verifier).not.toMatch(
      /Remove-Item[\s\S]{0,400}?-Path[\s\S]{0,400}?AudioBud-[^\r\n]{0,200}-updater-\*/,
    );
    expect(verifier).toContain("cleanupErrors");
    expect(verifier).toContain("Trusted root certificate cleanup failed");
    expect(verifier).toContain("Candidate server cleanup failed");
    expect(verifier).toContain("updater-prepublication-evidence.json");
    expect(workflow).toContain(
      "audiobud-updater-prepublication-$env:RELEASE_TAG-$env:GITHUB_RUN_ATTEMPT",
    );
    const verificationStep = stepBlock(
      "Apply private updater and preserve user data",
    );
    expect(verificationStep.indexOf("UPDATER_EVIDENCE_DIRECTORY")).toBeLessThan(
      verificationStep.indexOf("& scripts/verify-updater-candidate.ps1"),
    );
    const evidenceUploadStep = stepBlock(
      "Upload private updater verification evidence",
    );
    expect(evidenceUploadStep).toContain("always()");
    expect(evidenceUploadStep).toContain(
      "steps.updater-meta.outputs.evidence_artifact != ''",
    );
    expect(evidenceUploadStep).toContain(
      "env.UPDATER_EVIDENCE_DIRECTORY != ''",
    );
    expect(workflow).toContain(
      "- name: Download private updater verification evidence",
    );
    expect(workflow).toContain("- name: Attest updater verification evidence");
    expect(workflow).toContain("updater-prepublication-evidence.json");
    const attestEvidence = stepPosition("Attest updater verification evidence");
    const uploadDraft = stepPosition(
      "Upload macOS artifacts to joint draft release",
    );
    expect(attestEvidence).toBeLessThan(uploadDraft);
    const draftUpload = stepBlock(
      "Upload macOS artifacts to joint draft release",
    );
    expect(draftUpload).toContain('"$EVIDENCE" --clobber');
    expect(draftUpload).toContain(
      '--pattern "updater-prepublication-evidence.json"',
    );
    expect(draftUpload).toContain("cmp --silent");
    expect(draftUpload.indexOf("gh release upload")).toBeLessThan(
      draftUpload.indexOf("cmp --silent"),
    );
    expect(ciWorkflow).toContain("Parse updater candidate verifier");
    expect(ciWorkflow).toContain(
      "[System.Management.Automation.Language.Parser]::ParseFile",
    );
  });

  test("checks the packaged NSIS uninstall version before release", () => {
    const packaging = stepBlock("Verify packaged application signatures");

    expect(packaging).toContain("function Get-OptionalRegistryStringValue");
    const optionalValueReader = packaging.slice(
      packaging.indexOf("function Get-OptionalRegistryStringValue"),
      packaging.indexOf("function Assert-AudioBudUninstallRegistration"),
    );
    expect(optionalValueReader).toContain("if ($null -eq $Values)");
    expect(optionalValueReader.indexOf("if ($null -eq $Values)")).toBeLessThan(
      optionalValueReader.indexOf("$Values.PSObject.Properties[$Name]"),
    );
    expect(packaging).toContain(
      "Get-ItemProperty -LiteralPath $key.PSPath -ErrorAction Stop",
    );
    expect(packaging).not.toContain("Get-ItemPropertyValue");
    expect(packaging).toContain("Assert-AudioBudUninstallRegistration");
    expect(packaging).toContain("-Name 'DisplayVersion'");
    expect(packaging).toContain("-ExpectedVersion $env:APP_VERSION");
    expect(packaging).toContain("-InstallDirectory $nsisDirectory");
    expect(packaging).toContain("param([string] $Path)");
    expect(packaging).toContain(".Trim('\"')");
  });
});
