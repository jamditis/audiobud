import { test, expect } from "bun:test";
import { wer } from "./wer";

test("identical strings have 0 WER", () => {
  expect(wer("the quick brown fox", "the quick brown fox")).toBe(0);
});

test("one substitution in four words is 0.25", () => {
  expect(wer("the quick brown fox", "the quick green fox")).toBeCloseTo(0.25, 5);
});

test("case and punctuation are normalized", () => {
  expect(wer("Hello, world.", "hello world")).toBe(0);
});
