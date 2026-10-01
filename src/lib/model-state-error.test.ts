import { describe, expect, it } from "bun:test";
import type { TFunction } from "i18next";
import {
  localizeModelLoadError,
  modelLoadingFailureMessage,
  WHISPER_MODEL_PATH_NOT_UTF8,
} from "./model-state-error";

const fakeT = ((key: string) => `translated:${key}`) as unknown as TFunction;

describe("modelLoadingFailureMessage", () => {
  it("uses the localized fallback when the backend omits technical detail", () => {
    expect(modelLoadingFailureMessage(null, "Erreur du modèle")).toBe(
      "Erreur du modèle",
    );
    expect(modelLoadingFailureMessage(undefined, "Modellfehler")).toBe(
      "Modellfehler",
    );
    expect(modelLoadingFailureMessage("", "Error del modelo")).toBe(
      "Error del modelo",
    );
  });

  it("preserves backend detail when one was supplied", () => {
    expect(
      modelLoadingFailureMessage("Model path is unavailable", "Model error"),
    ).toBe("Model path is unavailable");
  });
});

describe("localizeModelLoadError", () => {
  it("translates the non-UTF-8 Whisper path code", () => {
    expect(localizeModelLoadError(WHISPER_MODEL_PATH_NOT_UTF8, fakeT)).toBe(
      "translated:errors.whisperModelPathNotUtf8",
    );
  });

  it("passes other backend detail through unchanged", () => {
    expect(localizeModelLoadError("Model not downloaded", fakeT)).toBe(
      "Model not downloaded",
    );
    expect(localizeModelLoadError("constructor", fakeT)).toBe("constructor");
    expect(localizeModelLoadError(null, fakeT)).toBeNull();
  });
});
