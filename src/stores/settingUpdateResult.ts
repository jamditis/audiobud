/**
 * Classify the result of a settings updater command.
 *
 * tauri-specta command bindings resolve to `{ status: "ok" | "error" }` and only
 * throw for transport-level failures, so a Rust `Err` (for example a declined
 * external-script confirmation dialog) resolves rather than throwing. Callers
 * that rely on a thrown error to roll back an optimistic update would otherwise
 * silently treat the failure as success.
 *
 * Returns the error message when the result signals failure, or `null` otherwise.
 * Plain (non-Result) command return values are treated as success.
 */
export function settingUpdateError(result: unknown): string | null {
  if (
    result !== null &&
    typeof result === "object" &&
    "status" in result &&
    (result as { status: unknown }).status === "error"
  ) {
    const { error } = result as { error?: unknown };
    return typeof error === "string" && error.length > 0
      ? error
      : "Setting was not saved";
  }
  return null;
}
