import { describe, expect, test } from "bun:test";
import {
  parakeetInputTooLongSeconds,
  recordingDurationLabel,
  transcriptionTimeoutSeconds,
} from "./transcription-error";

describe("transcription timeout errors", () => {
  test("recognizes the retry command's watchdog timeout", () => {
    expect(
      transcriptionTimeoutSeconds(
        new Error("Transcription timed out after 45s"),
      ),
    ).toBe(45);
    expect(
      transcriptionTimeoutSeconds("Transcription timed out after 45s"),
    ).toBe(45);
  });

  test("does not suppress unrelated retry failures", () => {
    expect(transcriptionTimeoutSeconds(new Error("Model is not loaded"))).toBe(
      null,
    );
    expect(transcriptionTimeoutSeconds("Transcription timed out soon")).toBe(
      null,
    );
  });
});

describe("Parakeet input-length errors", () => {
  test("recognizes the backend's stable length-limit contract", () => {
    expect(parakeetInputTooLongSeconds("parakeet_input_too_long:391")).toBe(
      391,
    );
    expect(
      parakeetInputTooLongSeconds(new Error("parakeet_input_too_long:650")),
    ).toBe(650);
    expect(
      parakeetInputTooLongSeconds("Parakeet input is too long"),
    ).toBeNull();
  });

  test("formats the recording duration for localized copy", () => {
    expect(recordingDurationLabel(391)).toBe("6:31");
    expect(recordingDurationLabel(650)).toBe("10:50");
  });
});
