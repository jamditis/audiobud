// Stage the Windows runtime DLLs that audiobud.exe load-time-imports next to the
// freshly built executable so both installers ship them and the app launches on a
// clean machine that lacks the VC++ Redistributable or a driver-supplied Vulkan
// loader. The MSI bundler harvests the binary's sibling DLLs automatically; the
// NSIS bundler does not, so the custom template (src-tauri/nsis/installer.nsi)
// adds them explicitly with File.
//
// Runs as Tauri's beforeBundleCommand: after the Rust build produces the exe and
// before the installers are assembled. No-op on non-Windows so macOS and Linux
// builds are unaffected. Fails the build (rather than shipping an installer that
// will not launch) if any source DLL cannot be located.
//
// Sources:
//   - VC++ CRT (msvcp140, msvcp140_1, vcruntime140, vcruntime140_1): copied from
//     System32, which on a build host with the VS toolchain holds the
//     redistributable copies. App-local deployment of these is supported.
//   - vulkan-1.dll: the Vulkan loader ships with GPU drivers, not the SDK, so CI
//     fetches the LunarG runtime components and points VULKAN_RUNTIME_DLL at the
//     extracted x64 loader. A driver-equipped dev box falls back to System32.
//
// This script also verifies (does not stage) DirectML.dll: it is produced next to
// the exe by the ort-directml build, so the MSI harvests it as a sibling and the
// NSIS template Files it for parity. Verifying it here fails the build with a clear
// message if the build did not produce it, instead of a cryptic makensis "File:
// could not find" later. See issue #44.

import { copyFileSync, existsSync, statSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import process from "node:process";

// Tauri sets TAURI_ENV_PLATFORM to the target's Rust target_os ("windows",
// "darwin", "linux", ...). Gate the no-op on the TARGET platform, not the host, so
// a Windows bundle cross-built from Linux/macOS still attempts to stage rather than
// silently skipping and leaving the NSIS template to File missing DLLs. Fall back
// to the host platform when the var is absent (e.g. invoked outside Tauri).
const targetPlatform =
  process.env.TAURI_ENV_PLATFORM ??
  (process.platform === "win32" ? "windows" : process.platform);
if (targetPlatform !== "windows") {
  console.log(
    `[bundle-runtime-dlls] target platform ${targetPlatform} is not Windows, nothing to stage`,
  );
  process.exit(0);
}

// The VC++ CRT DLLs are sourced from the host's System32, so producing a Windows
// bundle requires a Windows host. A Windows target on a non-Windows host cannot
// supply them; fail with a clear message instead of letting the NSIS template fail
// later on missing File entries.
if (process.platform !== "win32") {
  console.error(
    `[bundle-runtime-dlls] building a Windows bundle needs a Windows host to ` +
      `source the VC++ runtime DLLs from System32; cross-building from ` +
      `${process.platform} is not supported.`,
  );
  process.exit(1);
}

const system32 = join(process.env.SystemRoot ?? "C:\\Windows", "System32");

// tauri build emits to <target>/<profile>, where profile is "release" or, for
// `tauri build --debug`, "debug" (Tauri sets TAURI_ENV_DEBUG for the hook). With
// an explicit --target it nests the triple as <target>/<triple>/<profile>. CI
// shortens <target> via CARGO_TARGET_DIR to keep cmake paths under MAX_PATH. Probe
// both layouts and use whichever holds the exe so targeted builds (Tauri sets
// TAURI_ENV_TARGET_TRIPLE for the hook) work too.
const profile = process.env.TAURI_ENV_DEBUG === "true" ? "debug" : "release";
const targetDir = process.env.CARGO_TARGET_DIR ?? join("src-tauri", "target");
const triple = process.env.TAURI_ENV_TARGET_TRIPLE;
const candidateOutDirs = [
  triple ? join(targetDir, triple, profile) : null,
  join(targetDir, profile),
].filter(Boolean);

const outDir = candidateOutDirs.find((dir) =>
  existsSync(join(dir, "audiobud.exe")),
);
if (!outDir) {
  console.error(
    `[bundle-runtime-dlls] built exe not found; looked for audiobud.exe in: ` +
      `${candidateOutDirs.join(", ")}. The hook ran before the build ` +
      "produced it, or the output path differs.",
  );
  process.exit(1);
}

// Each entry lists the candidate source paths to try in order.
const dlls = [
  { name: "msvcp140.dll", sources: [join(system32, "msvcp140.dll")] },
  { name: "msvcp140_1.dll", sources: [join(system32, "msvcp140_1.dll")] },
  { name: "vcruntime140.dll", sources: [join(system32, "vcruntime140.dll")] },
  {
    name: "vcruntime140_1.dll",
    sources: [join(system32, "vcruntime140_1.dll")],
  },
  {
    name: "vulkan-1.dll",
    sources: [process.env.VULKAN_RUNTIME_DLL, join(system32, "vulkan-1.dll")],
  },
];

let failed = false;
for (const dll of dlls) {
  const source = dll.sources.find(
    (candidate) => candidate && existsSync(candidate),
  );
  if (!source) {
    const looked = dll.sources.filter(Boolean).join(", ") || "(no candidates)";
    console.error(
      `[bundle-runtime-dlls] ${dll.name} not found; looked in: ${looked}`,
    );
    failed = true;
    continue;
  }
  const dest = join(outDir, dll.name);
  copyFileSync(source, dest);
  console.log(
    `[bundle-runtime-dlls] ${dll.name} <- ${source} (${statSync(dest).size} bytes)`,
  );
}

// Stage the third-party license notices next to the exe so both installers ship
// them alongside the redistributed runtime DLLs they cover (issue #45): the MSI
// harvests it as a sibling of the exe and the NSIS template Files it explicitly.
// Resolve from the repo root (one level up from this script) so it does not
// depend on the process working directory.
const noticesSource = fileURLToPath(
  new URL("../THIRD_PARTY_NOTICES.md", import.meta.url),
);
if (!existsSync(noticesSource)) {
  console.error(
    `[bundle-runtime-dlls] THIRD_PARTY_NOTICES.md not found at ${noticesSource}; ` +
      "the installers redistribute runtime DLLs and must ship their license " +
      "notices. Restore the file at the repo root.",
  );
  failed = true;
} else {
  const noticesDest = join(outDir, "THIRD_PARTY_NOTICES.md");
  copyFileSync(noticesSource, noticesDest);
  console.log(
    `[bundle-runtime-dlls] THIRD_PARTY_NOTICES.md <- ${noticesSource} ` +
      `(${statSync(noticesDest).size} bytes)`,
  );
}

// DLLs the build already places next to the exe (so the MSI harvests them as
// siblings), which this script does NOT stage but the NSIS template Files for
// MSI/NSIS parity. Verify they are present so a missing build product fails here
// with a clear message rather than later in makensis. Copying them would be a
// self-copy (source and destination are the same path), so verify only.
const requiredSiblings = ["DirectML.dll"];
for (const name of requiredSiblings) {
  const siblingPath = join(outDir, name);
  if (!existsSync(siblingPath)) {
    console.error(
      `[bundle-runtime-dlls] ${name} not found next to the exe at ${siblingPath}; ` +
        "the NSIS installer Files it for MSI parity but the build did not " +
        "produce it. Check the ort-directml build step.",
    );
    failed = true;
    continue;
  }
  console.log(
    `[bundle-runtime-dlls] ${name} present next to the exe (${statSync(siblingPath).size} bytes)`,
  );
}

if (failed) {
  console.error(
    "[bundle-runtime-dlls] missing runtime DLLs; failing the build so a " +
      "non-launching or inconsistent installer is never produced.",
  );
  process.exit(1);
}
console.log(
  "[bundle-runtime-dlls] all runtime DLLs staged and required siblings verified",
);
