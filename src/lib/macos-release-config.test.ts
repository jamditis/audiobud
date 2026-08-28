import { describe, expect, it } from "bun:test";
import { existsSync, readdirSync, readFileSync } from "node:fs";

const read = (path: string) => readFileSync(path, "utf8");

describe("macOS release configuration", () => {
  it("uses one macOS 11 deployment floor", () => {
    const config = JSON.parse(read("src-tauri/tauri.conf.json"));

    expect(config.bundle.macOS.minimumSystemVersion).toBe("11.0");
    expect(read("src-tauri/build.rs")).toContain("arm64-apple-macosx11.0");
  });

  it("keeps the Rust updater dependency and plugin on Windows", () => {
    const cargo = read("src-tauri/Cargo.toml");
    const backend = read("src-tauri/src/lib.rs");
    const sharedDesktopDependencies = cargo.match(
      /\[target\.'cfg\(not\(any\(target_os = "android", target_os = "ios"\)\)\)'\.dependencies\]([\s\S]*?)\n\[target\./,
    )?.[1];
    const windowsDependencies = cargo.match(
      /\[target\.'cfg\(windows\)'\.dependencies\]([\s\S]*?)\n\[target\./,
    )?.[1];

    expect(sharedDesktopDependencies).toBeDefined();
    expect(sharedDesktopDependencies).not.toContain("tauri-plugin-updater");
    expect(windowsDependencies).toContain("tauri-plugin-updater");
    expect(backend).toMatch(
      /#\[cfg\(target_os = "windows"\)\]\s+use tauri_plugin_updater::UpdaterExt;/,
    );
    expect(backend).toMatch(
      /#\[cfg\(target_os = "windows"\)\][\s\S]*?builder = builder\.plugin\(tauri_plugin_updater/,
    );
  });

  it("loads the frontend updater API only when an update check runs", () => {
    const checker = read("src/components/update-checker/UpdateChecker.tsx");

    expect(checker).not.toMatch(/^import .*@tauri-apps\/plugin-updater.*$/m);
    expect(checker).toContain('await import("@tauri-apps/plugin-updater")');
  });

  it("has no repeated desktop capability permissions", () => {
    const capabilityPaths = readdirSync("src-tauri/capabilities")
      .filter((name) => name.endsWith(".json"))
      .map((name) => `src-tauri/capabilities/${name}`);
    const permissions = capabilityPaths.flatMap((path) => {
      const capability = JSON.parse(read(path));
      const identifiers = capability.permissions.filter(
        (permission: unknown): permission is string =>
          typeof permission === "string",
      );

      expect(new Set(identifiers).size, path).toBe(identifiers.length);
      return identifiers;
    });

    expect(
      permissions.filter((permission) => permission === "updater:default"),
    ).toHaveLength(1);
  });

  it("has no unused direct rdev dependency", () => {
    expect(read("src-tauri/Cargo.toml")).not.toMatch(/^rdev\s*=/m);
  });

  it("pins every Git dependency to the reviewed lockfile revision", () => {
    const cargo = read("src-tauri/Cargo.toml");
    const gitDependencies = cargo
      .split("\n")
      .filter((line) => line.includes('git = "'));

    expect(gitDependencies).not.toHaveLength(0);
    for (const dependency of gitDependencies) {
      expect(dependency).toContain("rev =");
      expect(dependency).not.toContain("branch =");
    }
  });

  it("does not compile the unreachable voice-command prototype", () => {
    expect(read("src-tauri/src/lib.rs")).not.toMatch(/^mod command;$/m);
    expect(existsSync("src-tauri/src/command.rs")).toBe(false);
  });

  it("uses a conditional release action and no fallback app version", () => {
    const footer = read("src/components/footer/Footer.tsx");

    expect(footer).not.toContain('setVersion("0.1.2")');
    expect(footer).toContain('currentPlatform === "macos"');
    expect(footer).toContain("RELEASES_URL");
    expect(footer).toContain("hasReleaseAction && version");
  });

  it("localizes the custom model create action", () => {
    const modelSelect = read(
      "src/components/settings/PostProcessingSettingsApi/ModelSelect.tsx",
    );

    expect(modelSelect).toContain('t("common.create")');
    expect(modelSelect).not.toContain('`Use "${input}"`');
  });

  it("documents the working Tauri bundle option syntax", () => {
    const build = read("BUILD.md");

    expect(build).toContain("bun run tauri build --bundles");
    expect(build).not.toContain("bun run tauri build -- --bundles");
  });
});
