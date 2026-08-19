import { describe, expect, test } from "bun:test";
import { historyEntryText } from "./history-entry-text";

describe("history entry text", () => {
  test("uses the processed output that was delivered", () => {
    expect(
      historyEntryText({
        transcription_text: "spoken draft",
        post_processed_text: "Finished output.",
      }),
    ).toBe("Finished output.");
  });

  test("falls back to the spoken transcript without processed output", () => {
    expect(
      historyEntryText({
        transcription_text: "spoken draft",
        post_processed_text: null,
      }),
    ).toBe("spoken draft");
  });
});
