import { describe, it, expect } from "bun:test";
import { parseWordList, CUSTOM_WORDS_CAP } from "../src/lib/wordList";

describe("parseWordList", () => {
  it("adds newline- and comma-separated words", () => {
    const r = parseWordList("foo\nbar, baz", []);
    expect(r.toAdd).toEqual(["foo", "bar", "baz"]);
    expect(r.addedCount).toBe(3);
  });

  it("keeps phrases up to three words intact", () => {
    const r = parseWordList("New York\nSan Francisco Bay", []);
    expect(r.toAdd).toEqual(["New York", "San Francisco Bay"]);
  });

  it("splits entries longer than three words into single words", () => {
    const r = parseWordList("one two three four", []);
    expect(r.toAdd).toEqual(["one", "two", "three", "four"]);
  });

  it("dedupes case-insensitively against the existing list", () => {
    const r = parseWordList("foo\nbar", ["Foo"]);
    expect(r.toAdd).toEqual(["bar"]);
    expect(r.duplicateCount).toBe(1);
  });

  it("dedupes within the imported batch", () => {
    const r = parseWordList("foo\nFOO\nbar", []);
    expect(r.toAdd).toEqual(["foo", "bar"]);
    expect(r.duplicateCount).toBe(1);
  });

  it("strips disallowed characters", () => {
    const r = parseWordList('fo<o>\n"bar"', []);
    expect(r.toAdd).toEqual(["foo", "bar"]);
  });

  it("skips blank lines", () => {
    const r = parseWordList("foo\n\n   \nbar", []);
    expect(r.addedCount).toBe(2);
  });

  it("drops single words longer than 50 chars as invalid", () => {
    const long = "a".repeat(51);
    const r = parseWordList(`${long}\nok`, []);
    expect(r.toAdd).toEqual(["ok"]);
    expect(r.invalidCount).toBe(1);
  });

  it("enforces the cap and reports the overflow", () => {
    const existing = Array.from({ length: 499 }, (_, i) => `w${i}`);
    const r = parseWordList("a\nb\nc", existing, CUSTOM_WORDS_CAP);
    expect(r.addedCount).toBe(1);
    expect(r.overCapCount).toBe(2);
  });

  it("defaults the cap to 500", () => {
    expect(CUSTOM_WORDS_CAP).toBe(500);
  });
});
