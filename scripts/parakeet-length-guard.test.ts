import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";

const transcription = readFileSync(
  "src-tauri/src/managers/transcription.rs",
  "utf8",
);
const actions = readFileSync("src-tauri/src/actions.rs", "utf8");
const app = readFileSync("src/App.tsx", "utf8");
const historySettings = readFileSync(
  "src/components/settings/history/HistorySettings.tsx",
  "utf8",
);
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

  test("emits a model-load failure when the selected model is missing", () => {
    const missingModelStart = transcription.indexOf(
      "let model_info =",
      transcription.indexOf("pub fn load_model"),
    );
    const notDownloaded = transcription.indexOf(
      "if !model_info.is_downloaded",
      missingModelStart,
    );

    expect(missingModelStart).toBeGreaterThan(-1);
    expect(notDownloaded).toBeGreaterThan(missingModelStart);

    const missingModelBranch = transcription.slice(
      missingModelStart,
      notDownloaded,
    );
    expect(missingModelBranch).toContain('event_type: "loading_failed"');
    expect(missingModelBranch).toContain("Model not found");
  });

  test("emits a model-load failure when its local path cannot be resolved", () => {
    const pathLookup = transcription.indexOf(
      "get_model_path(model_id)",
      transcription.indexOf("pub fn load_model"),
    );
    const engineLoad = transcription.indexOf(
      "// Create appropriate engine based on model type",
      pathLookup,
    );

    expect(pathLookup).toBeGreaterThan(-1);
    expect(engineLoad).toBeGreaterThan(pathLookup);

    const pathFailureBranch = transcription.slice(pathLookup, engineLoad);
    expect(pathFailureBranch).toContain("emit_loading_failed");
  });

  test("keeps backend details out of live and retry error toasts", () => {
    const liveErrorStart = app.indexOf("listen<TranscriptionErrorEvent>(");
    const modelErrorStart = app.indexOf(
      "// Listen for model loading failures",
      liveErrorStart,
    );
    const liveErrorHandler = app.slice(liveErrorStart, modelErrorStart);

    expect(liveErrorHandler).toContain("classifyTranscriptionError");
    expect(liveErrorHandler).toContain('t("errors.transcriptionErrorGeneric")');
    expect(liveErrorHandler).not.toContain("? event.payload.message");

    const retryStart = historySettings.indexOf("const handleRetranscribe");
    const retryEnd = historySettings.indexOf("const formattedDate", retryStart);
    const retryHandler = historySettings.slice(retryStart, retryEnd);

    expect(retryHandler).toContain('t("errors.transcriptionErrorGeneric")');
    expect(retryHandler).not.toContain(
      "error instanceof Error ? error.message",
    );
  });
});
