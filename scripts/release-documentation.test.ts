import { describe, expect, it } from "bun:test";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

const root = join(import.meta.dir, "..");
const read = (path: string) => readFileSync(join(root, path), "utf8");
const compact = (value: string) => value.replace(/\s+/g, " ");

describe("v0.6.0 release documentation", () => {
  it("keeps one draft changelog entry and complete draft release notes", () => {
    expect(existsSync(join(root, "RELEASE_NOTES.md"))).toBe(true);

    const changelog = compact(read("CHANGELOG.md"));
    const notes = compact(read("RELEASE_NOTES.md"));

    expect(changelog).toContain("## 0.6.0 - Unreleased");
    expect(changelog).toContain("Apple Silicon");
    expect(changelog).toContain("signed and notarized");
    expect(notes).toContain("# AudioBud v0.6.0 draft release notes");
    expect(notes).toContain("AudioBud v0.6.0 is not published yet");
    expect(notes).toContain("Apple Silicon Macs running macOS 11 or later");
    expect(notes).toContain("macOS updates are manual");
    expect(notes).toContain("Microsoft Store still serves v0.4.4");
    expect(notes).toContain("AudioBud_<version>_macos_aarch64.dmg");
    expect(notes).toContain("SHA256SUMS-macos.txt");
    expect(notes).toContain("No Intel Mac build is included");
  });

  it("states the validated platform boundary", () => {
    const requirements = compact(read("SYSTEM_REQUIREMENTS.md"));

    expect(requirements).toContain("Windows 10 or 11 on x64");
    expect(requirements).toContain("macOS 11 or later on Apple Silicon");
    expect(requirements).toContain("Microphone and Accessibility permissions");
    expect(requirements).toContain(
      "Intel Mac and Linux builds are not validated",
    );
    expect(requirements).toContain("Apple Intelligence is optional");
  });

  it("makes the local model recommendation specific to each system", () => {
    const readme = compact(read("README.md"));
    const notes = compact(read("RELEASE_NOTES.md"));
    const home = compact(read("docs/index.html"));

    expect(readme).toContain(
      "Whisper Turbo is recommended on Apple Silicon Macs",
    );
    expect(readme).toContain("Parakeet V3 remains the Windows default");
    expect(notes).toContain("Whisper Turbo is the recommended Mac model");
    expect(home).toMatch(
      /<strong>Parakeet V3<\/strong>[\s\S]*?<span class="tag solid">Windows default<\/span>/,
    );
    expect(home).toMatch(
      /<strong>Whisper Turbo<\/strong>[\s\S]*?<span class="tag solid">Mac recommended<\/span>[\s\S]*?<span class="tag">Metal<\/span>/,
    );
    expect(home).not.toMatch(
      /<strong>Parakeet V3<\/strong>[\s\S]{0,300}<span class="tag solid">recommended<\/span>/,
    );
  });

  it("documents the macOS signing and release boundary without secret values", () => {
    expect(existsSync(join(root, "docs", "macos-release.md"))).toBe(true);

    const build = read("BUILD.md");
    const macRelease = read("docs/macos-release.md");
    const macBaseline = read("tasks/macos-baseline.md");
    const updater = read("docs/updater-signing.md");

    for (const variable of [
      "APPLE_API_KEY",
      "APPLE_API_ISSUER",
      "APPLE_API_PRIVATE_KEY",
      "APPLE_CERTIFICATE",
      "APPLE_CERTIFICATE_PASSWORD",
    ]) {
      expect(macRelease).toContain(variable);
    }
    expect(macRelease).toContain("Notarize and staple the DMG separately");
    expect(macRelease).toContain("artifact-signing");
    expect(build).toContain("docs/macos-release.md");
    expect(updater).toContain("Windows NSIS builds only");
    expect(updater).toContain("macOS uses manual updates");
    expect(macRelease).not.toContain("3Y74X55F58");
    expect(macRelease).not.toContain("7944b086-4ca7-4756-9719-602976a67775");
    expect(macBaseline).not.toMatch(/^- (issuer|key) id:/gim);
    expect(macBaseline).toContain(
      "Issuer and key identifiers are stored outside the repository",
    );
  });

  it("keeps the Store update plan accurate before Partner Center submission", () => {
    const store = compact(read("STORE_SUBMISSION.md"));

    expect(store).toContain("## v0.6.0 update submission");
    expect(store).toContain("Microsoft Store still serves v0.4.4");
    expect(store).toContain("immutable, versioned HTTPS URL");
    expect(store).toContain("Do not upload or submit before");
    expect(store).toContain("AudioBud_<version>_x64-setup.exe");
  });

  it("keeps one current release decision in the task record", () => {
    const todoSource = read("tasks/todo.md");
    const todo = compact(todoSource);

    expect(
      todoSource.match(/^#{2,3} (current )?release decision$/gim),
    ).toHaveLength(1);
    expect(todo).toContain(
      "current local macOS candidate passed the full source review",
    );
    expect(todo).toContain("explicit approval to commit and push");
    expect(todo).toContain("protected remote candidate build");
    expect(todo).toContain("clean-Mac tests");
    expect(todo).toContain("publication has separate approval");
    expect(todo).not.toContain("finish the independent review");
    expect(todo).not.toContain("complete phase 0");
  });

  it("keeps contributor guidance and source comments current for macOS", () => {
    const translations = read("CONTRIBUTING_TRANSLATIONS.md");
    const preflight = compact(read("src/lib/preflight.ts"));

    expect(translations).toContain("# Contributing translations to AudioBud");
    expect(translations).not.toMatch(/translate Handy|making Handy accessible/);
    expect(preflight).toContain("macOS is validated with soft checks");
    expect(preflight).not.toContain(
      "macOS and Linux are inherited from upstream Handy and not yet validated",
    );
  });

  it("keeps the application HTML metadata aligned with the public site", () => {
    const appHtml = read("index.html");

    expect(appHtml).toContain(
      "AudioBud - local dictation for Windows and macOS",
    );
    expect(appHtml).toContain(
      'content="AudioBud is a local-first dictation app for Windows and macOS.',
    );
    expect(appHtml).toContain('href="https://audiobud.amditis.tech/"');
    expect(appHtml).toContain(
      'content="AudioBud local dictation for Windows and macOS app interface"',
    );
    expect(appHtml).not.toContain("https://jamditis.github.io/audiobud");
  });
});
