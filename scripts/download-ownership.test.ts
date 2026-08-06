import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";

const modelManager = readFileSync("src-tauri/src/managers/model.rs", "utf8");

describe("model download ownership", () => {
  test("the cleanup guard releases only its own flag under the shared lock order", () => {
    const helperStart = modelManager.indexOf("fn release_download_claim");
    const cleanupStart = modelManager.indexOf("struct DownloadCleanup");
    const managerStart = modelManager.indexOf("pub struct ModelManager");
    const helper = modelManager.slice(helperStart, cleanupStart);
    const cleanup = modelManager.slice(cleanupStart, managerStart);

    expect(helperStart).toBeGreaterThan(-1);
    expect(cleanupStart).toBeGreaterThan(-1);
    expect(managerStart).toBeGreaterThan(cleanupStart);
    expect(cleanup).toContain("cancel_flag: Arc<AtomicBool>");
    expect(cleanup).toContain("release_download_claim(");
    expect(helper).toContain("let mut models = available_models.lock()");
    expect(helper).toContain("let mut flags = cancel_flags.lock()");
    expect(helper).toContain("Arc::ptr_eq");
  });

  test("successful completion uses the same atomic ownership release", () => {
    const successStart = modelManager.indexOf(
      "// Success releases the same ownership",
    );
    const completionEvent = modelManager.indexOf(
      "// Emit completion event",
      successStart,
    );
    const successCleanup = modelManager.slice(successStart, completionEvent);

    expect(successStart).toBeGreaterThan(-1);
    expect(completionEvent).toBeGreaterThan(successStart);
    expect(successCleanup).toContain("release_download_claim");
    expect(successCleanup).not.toContain("cancel_flags.lock().unwrap().remove");
  });

  test("status refresh checks active claims while holding the shared lock order", () => {
    const refreshStart = modelManager.indexOf("fn update_download_status");
    const refreshEnd = modelManager.indexOf(
      "fn auto_select_model_if_needed",
      refreshStart,
    );
    const refresh = modelManager.slice(refreshStart, refreshEnd);
    const modelsLock = refresh.indexOf("self.available_models.lock()");
    const flagsLock = refresh.indexOf("self.cancel_flags.lock()");

    expect(refreshStart).toBeGreaterThan(-1);
    expect(refreshEnd).toBeGreaterThan(refreshStart);
    expect(modelsLock).toBeGreaterThan(-1);
    expect(flagsLock).toBeGreaterThan(modelsLock);
  });

  test("a cancel observed after normal stream completion removes the completed partial", () => {
    const cancelStart = modelManager.indexOf(
      'info!("Download cancelled for: {} (before verification)", model_id)',
    );
    const verificationStart = modelManager.indexOf(
      "// Verify downloaded file size matches expected size",
      cancelStart,
    );
    const cancelBranch = modelManager.slice(cancelStart, verificationStart);

    expect(cancelStart).toBeGreaterThan(-1);
    expect(verificationStart).toBeGreaterThan(cancelStart);
    expect(cancelBranch).toContain("fs::remove_file(&partial_path)");
    expect(cancelBranch).not.toContain("total_size > 0");
  });
});
