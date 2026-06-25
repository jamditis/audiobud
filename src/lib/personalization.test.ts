import { expect, test } from "bun:test";
import { hasStoredPersonalizationData } from "./personalization";

test("false for null/undefined/empty", () => {
  expect(hasStoredPersonalizationData(null)).toBe(false);
  expect(hasStoredPersonalizationData(undefined)).toBe(false);
  expect(
    hasStoredPersonalizationData({
      learned_words: [],
      dismissed_suggestions: [],
    }),
  ).toBe(false);
});

test("true when learned_words present", () => {
  expect(hasStoredPersonalizationData({ learned_words: ["frobnicate"] })).toBe(
    true,
  );
});

test("true when only dismissed_suggestions present", () => {
  expect(
    hasStoredPersonalizationData({
      learned_words: [],
      dismissed_suggestions: ["bar"],
    }),
  ).toBe(true);
});

test("true when only learned_replacements present", () => {
  expect(
    hasStoredPersonalizationData({
      learned_words: [],
      learned_replacements: [{ from: "lite coin", to: "Litecoin" }],
      dismissed_suggestions: [],
    }),
  ).toBe(true);
});
