import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";

const transcription = readFileSync(
  "src-tauri/src/managers/transcription.rs",
  "utf8",
);
const actions = readFileSync("src-tauri/src/actions.rs", "utf8");
const engineLimits = readFileSync(
  "src-tauri/src/managers/engine_limits.rs",
  "utf8",
);

describe("Parakeet input-length refusal", () => {
  test("honors immediate model unloading before returning the refusal", () => {
    const guardStart = transcription.indexOf(
      "if matches!(engine, LoadedEngine::Parakeet(_))",
    );
    const engineCall = transcription.indexOf(
      "let transcribe_result = catch_unwind",
      guardStart,
    );

    expect(guardStart).toBeGreaterThan(-1);
    expect(engineCall).toBeGreaterThan(guardStart);

    const guard = transcription.slice(guardStart, engineCall);
    const restore = guard.indexOf("self.engine.try_restore");
    const unload = guard.indexOf(
      'self.maybe_unload_immediately("parakeet length refusal")',
    );
    const refusal = guard.indexOf("return Err(anyhow::anyhow!(message))");

    expect(restore).toBeGreaterThan(-1);
    expect(unload).toBeGreaterThan(restore);
    expect(refusal).toBeGreaterThan(unload);
  });

  test("keeps shared error contracts outside the CI-swapped manager", () => {
    for (const constant of [
      "MODEL_AUTO_LOAD_FAILED_ERROR",
      "MODEL_NOT_LOADED_ERROR",
      "WEDGED_ENGINE_ERROR",
    ]) {
      expect(engineLimits).toContain(`const ${constant}`);
      expect(actions).toContain(constant);
      expect(transcription).not.toContain(`const ${constant}`);
    }
  });
});
