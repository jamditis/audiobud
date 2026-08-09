import { describe, expect, it } from "bun:test";
import { modelLoadingFailureMessage } from "./model-state-error";

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
