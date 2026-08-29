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
    expect(build).toContain(
      "/Applications/Xcode_26.0.1.app/Contents/Developer",
    );
    expect(build).toContain("SDKROOT");
    expect(build).toContain("FoundationModels.framework");
    expect(build).toContain("validate-sbom-file-checksums.ts");
    expect(build).toContain("placeholder value");
    expect(macRelease).toContain(
      "/Applications/Xcode_26.0.1.app/Contents/Developer",
    );
    expect(macRelease).toContain("macOS 26 SDK");
    expect(macRelease).toContain("SDKROOT");
    expect(macRelease).toContain("FoundationModels.framework");
    expect(macRelease).toContain("validate-sbom-file-checksums.ts");
    expect(macRelease).toContain("placeholder");
    expect(macRelease).toContain("final app binary");
    expect(updater).toContain("Windows NSIS builds only");
    expect(updater).toContain("macOS uses manual updates");
    expect(macRelease).not.toContain("3Y74X55F58");
    expect(macRelease).not.toContain("7944b086-4ca7-4756-9719-602976a67775");
    expect(macBaseline).not.toMatch(/^- (issuer|key) id:/gim);
    expect(macBaseline).toContain(
      "Issuer and key identifiers are stored outside the repository",
    );
    expect(macBaseline).toContain(
      "481 frontend tests and 2,180 assertions across 45 files",
    );
    expect(macBaseline).toContain("503 Rust library tests");
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
    expect(todo).toContain("local and remote `main` point to `e417154`");
    expect(todo).toContain("Candidate run `33195432598` passed");
    expect(todo).toContain(
      "ac1ecc5661473f4fe7533cd971df5c91b654e1a1848a543dcfcdf7534f49f566",
    );
    expect(todo).toContain("The candidate is rejected");
    expect(todo).toContain("all 67 Windows SBOM file records");
    expect(todo).toContain("dedicated release-fix pull request");
    expect(todo).toContain("Pull requests 311 through 317 stay outside v0.6.0");
    expect(todo).toContain("clean Apple Silicon Mac");
    expect(todo).toContain("Do not create or push the v0.6.0 tag");
    expect(todo).toContain("Session handoff for August 28, 2026");
    expect(todo).toContain("before their later gates and approvals");
    expect(todo).toContain("Partner Center");
    expect(todo).not.toContain("pull request 309 is open");
    expect(todo).not.toContain("follow-up fixes are local and uncommitted");
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
