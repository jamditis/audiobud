import { describe, expect, it } from "bun:test";
import { readFileSync } from "node:fs";

const engine = readFileSync(".github/workflows/engine.yml", "utf8");
const ci = readFileSync(".github/workflows/ci.yml", "utf8");
const release = readFileSync(".github/workflows/release.yml", "utf8");

// These workflows explain themselves at length, and engine.yml names the mock
// swap it exists to avoid. Assertions about what a job *does* have to read the
// directives, or a comment describing the forbidden step fails the check.
const directives = (yaml: string) =>
  yaml
    .split("\n")
    .filter((line) => !/^\s*#/.test(line))
    .join("\n");

const engineSteps = directives(engine);

describe("engine workflow", () => {
  it("compiles the transcription manager the application actually ships", () => {
    // The point of this workflow is the absence of the mock swap. ci.yml
    // overwrites transcription.rs with a 130-line stub and deletes
    // transcribe-rs from the manifest; if either line ever appears here, the
    // job goes green while compiling nothing that ships.
    expect(engineSteps).not.toContain("transcription_mock.rs");
    expect(engineSteps).not.toMatch(/sed -i .*transcribe-rs/);
  });

  it("runs cargo test rather than only building", () => {
    // `cargo build` would catch a compile error but not a behavioural one, and
    // the 258 tests that do exist never run against the real engine otherwise.
    expect(engineSteps).toMatch(/run: cargo test/);
  });

  it("pins the same Vulkan SDK the release build links against", () => {
    // A different SDK here would let this job pass against toolchain the
    // release never sees, which is the failure mode it exists to prevent.
    const pin = /humbletim\/install-vulkan-sdk@([0-9a-f]{40})/;
    const version = /version: (1\.\d+\.\d+\.\d+)/;

    expect(pin.exec(engine)?.[1]).toBe(pin.exec(release)?.[1]);
    expect(version.exec(engine)?.[1]).toBe(version.exec(release)?.[1]);
  });

  it("matches the release build's SIMD posture", () => {
    // Building with wider SIMD than the release would compile a variant no
    // user runs, and would not fail on code the release rejects.
    for (const flag of [
      "GGML_NATIVE",
      "GGML_AVX",
      "GGML_AVX2",
      "GGML_FMA",
      "GGML_F16C",
    ]) {
      expect(engine).toMatch(new RegExp(`${flag}: "OFF"`));
      expect(release).toMatch(new RegExp(`${flag}: "OFF"`));
    }
  });

  it("runs whenever Rust changes, and on the workflow itself", () => {
    // A paths filter that misses the workflow file means a change to this job
    // cannot be tested by the pull request that makes it.
    expect(engine).toContain('- "src-tauri/**"');
    expect(engine).toContain('- ".github/workflows/engine.yml"');
  });

  it("keeps the fast mocked jobs in ci.yml and says where the real one lives", () => {
    // The mock is a deliberate speed trade, not an oversight. It stays -- but
    // the file has to point at the job that covers what it skips, because the
    // gap went unnoticed precisely while nothing wrote it down.
    expect(ci).toContain("transcription_mock.rs");
    expect(ci).toContain("engine.yml");
  });
});
