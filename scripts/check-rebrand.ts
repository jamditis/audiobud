// Asserts AudioBud's brand identity across config files. Exits non-zero on any mismatch.
import { readFileSync } from "node:fs";

const fail: string[] = [];

const tauri = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8"));
if (tauri.identifier !== "tech.amditis.audiobud")
  fail.push(`tauri.conf.json identifier = ${tauri.identifier} (want tech.amditis.audiobud)`);
if (tauri.productName !== "AudioBud")
  fail.push(`tauri.conf.json productName = ${tauri.productName} (want AudioBud)`);

const cargo = readFileSync("src-tauri/Cargo.toml", "utf8");
if (!/^name\s*=\s*"audiobud"\s*$/m.test(cargo))
  fail.push('Cargo.toml [package] name is not "audiobud"');
if (!/^default-run\s*=\s*"audiobud"\s*$/m.test(cargo))
  fail.push('Cargo.toml default-run is not "audiobud"');

const pkg = JSON.parse(readFileSync("package.json", "utf8"));
if (pkg.name !== "audiobud")
  fail.push(`package.json name = ${pkg.name} (want audiobud)`);

if (fail.length) {
  console.error("REBRAND CHECK FAILED:\n" + fail.map((f) => " - " + f).join("\n"));
  process.exit(1);
}
console.log("REBRAND CHECK PASSED");
