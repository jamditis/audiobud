import { describe, expect, test } from "bun:test";
import { settingUpdateError } from "./settingUpdateResult";

describe("settingUpdateError", () => {
  test("returns the error message for a failed tauri-specta result", () => {
    // A declined external-script confirmation dialog makes the Rust command
    // return Err, which the binding resolves as { status: "error" } rather than
    // throwing. updateSetting must treat this as a failure so it rolls back.
    expect(
      settingUpdateError({ status: "error", error: "confirmation declined" }),
    ).toBe("confirmation declined");
  });

  test("falls back to a generic message when error is empty or missing", () => {
    expect(settingUpdateError({ status: "error" })).toBe("Setting was not saved");
    expect(settingUpdateError({ status: "error", error: "" })).toBe(
      "Setting was not saved",
    );
  });

  test("returns null for a successful result", () => {
    expect(settingUpdateError({ status: "ok", data: null })).toBeNull();
  });

  test("treats plain (non-Result) command return values as success", () => {
    // Some commands return their value directly instead of a Result wrapper.
    expect(settingUpdateError(undefined)).toBeNull();
    expect(settingUpdateError(null)).toBeNull();
    expect(settingUpdateError(["a", "b"])).toBeNull();
    expect(settingUpdateError("ok")).toBeNull();
    expect(settingUpdateError(42)).toBeNull();
  });
});
