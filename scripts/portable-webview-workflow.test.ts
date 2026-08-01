import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";

const workflow = readFileSync(".github/workflows/release.yml", "utf8");
const config = JSON.parse(
  readFileSync("src-tauri/tauri.portable-webview.conf.json", "utf8"),
);
const nsisTemplate = readFileSync("src-tauri/nsis/installer.nsi", "utf8");
const gitignore = readFileSync(".gitignore", "utf8");

function stepPosition(name: string): number {
  const position = workflow.indexOf(`- name: ${name}`);
  expect(position, `Missing workflow step: ${name}`).toBeGreaterThan(-1);
  return position;
}

function stepBlock(name: string): string {
  const position = stepPosition(name);
  const next = workflow.indexOf("\n      - name:", position + 1);
  return workflow.slice(position, next === -1 ? undefined : next);
}

function nsisFunctionBlock(name: string): string {
  const position = nsisTemplate.indexOf(`Function ${name}`);
  expect(position, `Missing NSIS function: ${name}`).toBeGreaterThan(-1);
  const end = nsisTemplate.indexOf("FunctionEnd", position);
  expect(end, `Missing FunctionEnd for: ${name}`).toBeGreaterThan(position);
  return nsisTemplate.slice(position, end);
}

describe("self-contained portable WebView2 artifact", () => {
  test("keeps the fixed runtime in an opt-in config overlay", () => {
    expect(config).toEqual({
      $schema: "https://schema.tauri.app/config/2",
      bundle: {
        createUpdaterArtifacts: false,
        windows: {
          webviewInstallMode: {
            type: "fixedRuntime",
            path: "Microsoft.WebView2.FixedVersionRuntime.150.0.4078.105.x64",
          },
        },
      },
    });
    expect(gitignore).toContain(
      "src-tauri/Microsoft.WebView2.FixedVersionRuntime.*.x64/",
    );
  });

  test("downloads and verifies an exact Microsoft runtime", () => {
    expect(workflow).toContain("WEBVIEW2_FIXED_VERSION: 150.0.4078.105");
    expect(workflow).toContain(
      "WEBVIEW2_FIXED_CAB_SHA256: 26c07cad95615a672cde8c1843a326e18ad25d691f004347544e5e099bff9b92",
    );
    expect(workflow).toContain("WEBVIEW2_FIXED_CAB_BYTES: 297904860");
    const fetch = stepBlock("Fetch fixed WebView2 runtime");
    expect(fetch).toContain(
      "b401c036-cfb8-4dc4-a58e-8766441df4ac/Microsoft.WebView2.FixedVersionRuntime.150.0.4078.105.x64.cab",
    );
    expect(fetch).toContain("Get-FileHash");
    expect(fetch).toContain("Get-AuthenticodeSignature");
    expect(fetch).toContain("CN=Microsoft Corporation");
    expect(fetch).toContain("expand.exe");
  });

  test("uses a separate short target directory and emits a separate installer", () => {
    expect(workflow).toContain("PORTABLE_TARGET_DIR=");
    const build = stepBlock("Build fixed-runtime portable application");
    const bundle = stepBlock("Bundle fixed-runtime portable installer");
    expect(build).toContain("$env:CARGO_TARGET_DIR = $env:PORTABLE_TARGET_DIR");
    expect(build).toContain("src-tauri/tauri.portable-webview.conf.json");
    expect(bundle).toContain("--bundles nsis");
    expect(bundle).toContain("src-tauri/tauri.signing.conf.json");
    expect(bundle).toContain("src-tauri/tauri.portable-webview.conf.json");
    expect(workflow).toContain(
      "AudioBud_$($env:APP_VERSION)_x64-portable-webview-setup.exe",
    );
    const resolvedPath = stepBlock("Resolve fixed-runtime portable path");
    expect(resolvedPath).toContain(
      '$artifactDirectory = Join-Path $env:CARGO_TARGET_DIR "release"',
    );
    expect(resolvedPath).toContain(
      "Move-Item -LiteralPath $source -Destination $path",
    );
    expect(stepPosition("Resolve updater artifact paths")).toBeLessThan(
      stepPosition("Build fixed-runtime portable application"),
    );
  });

  test("applies the fixed-runtime ACLs and verifies portable invariants", () => {
    expect(nsisTemplate).toContain(
      '!define FIXEDWEBVIEW2DIRECTORY "Microsoft.WebView2.FixedVersionRuntime.150.0.4078.105.x64"',
    );
    expect(nsisTemplate).toContain("*S-1-15-2-2:(OI)(CI)(RX)");
    expect(nsisTemplate).toContain("*S-1-15-2-1:(OI)(CI)(RX)");
    expect(nsisTemplate).toContain("icacls.exe");

    const verification = stepBlock("Verify fixed-runtime portable install");
    expect(verification).toContain(
      '@("/S", "/PORTABLE", "/D=$installDirectory")',
    );
    expect(verification).toContain('"AudioBud Portable Mode"');
    expect(verification).toContain('Join-Path $installDirectory "Data"');
    expect(verification).toContain(
      'Join-Path $installDirectory "uninstall.exe"',
    );
    expect(verification).toContain(
      'Join-Path $runtimeDirectory "msedgewebview2.exe"',
    );
    expect(verification).toContain("S-1-15-2-2");
    expect(verification).toContain("S-1-15-2-1");
  });

  test("forces every fixed-runtime install into portable mode", () => {
    const installTypePage = nsisFunctionBlock("PageInstallType");
    expect(installTypePage).toMatch(
      /!if "\$\{INSTALLWEBVIEW2MODE\}" == ""\s+StrCpy \$PortableMode 1\s+Abort\s+!endif/,
    );

    const onInit = nsisFunctionBlock(".onInit");
    expect(onInit).toMatch(
      /!if "\$\{INSTALLWEBVIEW2MODE\}" == ""\s+StrCpy \$PortableMode 1\s+!else[\s\S]*\$CMDLINE "\/PORTABLE" \$PortableMode[\s\S]*!endif/,
    );
  });

  test("removes only superseded fixed runtimes during in-place upgrades", () => {
    const cleanup = nsisFunctionBlock("RemoveSupersededFixedRuntimes");
    expect(cleanup).toContain(
      'FindFirst $0 $1 "$INSTDIR\\Microsoft.WebView2.FixedVersionRuntime.*.x64"',
    );
    expect(cleanup).toContain('${If} $1 != "${FIXEDWEBVIEW2DIRECTORY}"');
    expect(cleanup).toContain('RMDir /r "$INSTDIR\\$1"');
    expect(cleanup).toContain("FindClose $0");
    expect(cleanup).not.toContain("Data");

    const installSection = nsisTemplate.slice(
      nsisTemplate.indexOf("Section Install"),
      nsisTemplate.indexOf(
        "SectionEnd",
        nsisTemplate.indexOf("Section Install"),
      ),
    );
    expect(installSection).toMatch(
      /!if "\$\{INSTALLWEBVIEW2MODE\}" == ""\s+Call RemoveSupersededFixedRuntimes\s+!endif/,
    );
    expect(installSection).toMatch(
      /!if "\$\{INSTALLWEBVIEW2MODE\}" == ""[\s\S]*icacls\.exe[\s\S]*!endif/,
    );
  });

  test("checksums, attests, and uploads the separate artifact", () => {
    for (const stepName of [
      "Write SHA256SUMS",
      "Attest release provenance",
      "Attest release SBOM",
      "Upload release artifacts to GitHub release",
      "Upload release artifacts as CI artifact",
    ]) {
      expect(stepBlock(stepName)).toContain(
        "steps.portable-webview-path.outputs.path",
      );
    }
    expect(stepBlock("Upload release artifacts as CI artifact")).not.toContain(
      "PORTABLE_TARGET_DIR",
    );
  });
});
