import { describe, expect, test } from "bun:test";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { resolve, join } from "node:path";
import { linuxBuildMessage, nativeTargetError } from "./native-platform";

const root = resolve(import.meta.dir, "..");
const read = (path: string) => readFileSync(resolve(root, path), "utf8");

describe("native platform boundary", () => {
  test("rejects native Linux targets before invoking Tauri", () => {
    for (const command of ["dev", "build", "bundle"]) {
      expect(nativeTargetError([command], "linux")).toBe(linuxBuildMessage);
      expect(
        nativeTargetError(
          [command, "--target", "aarch64-unknown-linux-gnu"],
          "darwin",
        ),
      ).toBe(linuxBuildMessage);
      expect(
        nativeTargetError(
          [command, "--target=x86_64-unknown-linux-gnu"],
          "win32",
        ),
      ).toBe(linuxBuildMessage);
      expect(
        nativeTargetError([command, "-t", "x86_64-unknown-linux-gnu"], "win32"),
      ).toBe(linuxBuildMessage);
      expect(
        nativeTargetError([command], "win32", "x86_64-unknown-linux-gnu"),
      ).toBe(linuxBuildMessage);
    }
  });

  test("preserves retained targets, help, and non-build commands", () => {
    for (const host of ["win32", "darwin"]) {
      expect(
        nativeTargetError(["build", "--no-bundle", "--ci"], host),
      ).toBeNull();
    }
    for (const target of [
      "aarch64-apple-darwin",
      "x86_64-apple-darwin",
      "x86_64-pc-windows-msvc",
    ]) {
      expect(
        nativeTargetError(["build", "--target", target], "linux"),
      ).toBeNull();
    }
    for (const args of [
      ["--version"],
      ["info"],
      ["build", "--help"],
      ["dev", "-h"],
    ]) {
      expect(nativeTargetError(args, "linux")).toBeNull();
    }
    expect(
      nativeTargetError(
        ["build", "--target", "aarch64-apple-darwin"],
        "darwin",
        "x86_64-unknown-linux-gnu",
      ),
    ).toBeNull();
    expect(
      nativeTargetError(
        ["dev", "--", "--target", "x86_64-unknown-linux-gnu"],
        "darwin",
      ),
    ).toBeNull();
  });

  test("keeps macOS and Windows configuration without Linux packaging", () => {
    const cargo = read("src-tauri/Cargo.toml");
    expect(cargo).not.toContain('cfg(target_os = "linux")');
    expect(cargo).not.toMatch(/^gtk(?:-layer-shell)?\s*=/m);
    expect(cargo).toContain('"whisper-metal"');
    expect(cargo).toContain('"ort-directml"');
    expect(cargo).toContain("tauri-nspanel");
    const config = JSON.parse(read("src-tauri/tauri.conf.json"));
    expect(config.identifier).toBe("tech.amditis.audiobud");
    expect(config.bundle.linux).toBeUndefined();
    expect(config.bundle.macOS.minimumSystemVersion).toBe("11.0");
    expect(config.bundle.macOS.hardenedRuntime).toBe(true);
    expect(config.bundle.windows.nsis.template).toBe("nsis/installer.nsi");
    const capability = JSON.parse(read("src-tauri/capabilities/desktop.json"));
    expect(capability.platforms).toEqual(["macOS", "windows"]);
  });

  test("does not restore Linux-only source branches or tray resources", () => {
    const inspect = (directory: string) => {
      for (const entry of readdirSync(directory, { withFileTypes: true })) {
        const path = join(directory, entry.name);
        if (entry.isDirectory()) inspect(path);
        else if (entry.name.endsWith(".rs")) {
          expect(readFileSync(path, "utf8")).not.toMatch(
            /#\[cfg(?:_attr)?\([^\]]*target_os\s*=\s*"linux"/,
          );
        }
      }
    };
    inspect(resolve(root, "src-tauri/src"));
    for (const path of [
      "src/components/settings/TypingTool.tsx",
      "src-tauri/resources/handy.png",
      "src-tauri/resources/recording.png",
      "src-tauri/resources/transcribing.png",
    ]) {
      expect(existsSync(resolve(root, path))).toBe(false);
    }
  });

  test("required shared check validates the real Mac engine and generated bindings", () => {
    const ci = read(".github/workflows/ci.yml");
    const shared = ci
      .split("  rust-tests:\n")[1]
      .split("  rust-tests-windows:\n")[0];
    expect(shared).toContain("name: Rust tests (mock transcription, no GPU)");
    expect(shared).toContain("runs-on: macos-15");
    expect(shared).toContain("cargo test --locked");
    expect(shared).toContain("--bin generate-bindings");
    expect(shared).toContain("git diff --exit-code -- ../src/bindings.ts");
    expect(shared).toContain("bun run tauri build --no-bundle --ci");
    expect(shared).not.toContain("transcription_mock.rs");
    expect(shared).not.toContain("artifact-signing");
    expect(ci).toContain("runs-on: ubuntu-latest");
    expect(read(".github/workflows/engine.yml")).toContain(
      "bun run tauri build --no-bundle --ci",
    );
  });
});
