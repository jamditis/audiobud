import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { createSliderDraft, createSliderInteraction } from "./sliderDraft";

describe("slider draft", () => {
  test("keeps rapid pointer values local until pointer release", () => {
    const committed: number[] = [];
    const draft = createSliderDraft(0.25, (value) => committed.push(value));

    draft.update(0.35);
    draft.update(0.55);
    draft.update(0.75);

    expect(draft.value()).toBe(0.75);
    expect(committed).toEqual([]);

    expect(draft.commit()).toBe(true);
    expect(committed).toEqual([0.75]);
  });

  test("commits a keyboard value once across key release and blur", () => {
    const committed: number[] = [];
    const draft = createSliderDraft(40, (value) => committed.push(value));

    draft.update(60);
    expect(draft.commit()).toBe(true);
    expect(draft.commit()).toBe(false);

    expect(committed).toEqual([60]);
  });

  test("syncs backend values without emitting a write", () => {
    const committed: number[] = [];
    const draft = createSliderDraft(40, (value) => committed.push(value));

    draft.sync(80);

    expect(draft.value()).toBe(80);
    expect(committed).toEqual([]);
  });

  test("restores the last committed value after pointer cancellation", () => {
    const committed: number[] = [];
    const draft = createSliderDraft(40, (value) => committed.push(value));

    draft.update(80);

    expect(draft.cancel()).toBe(40);
    expect(draft.value()).toBe(40);
    expect(committed).toEqual([]);
  });

  test("connects commits to pointer release, keyboard release, and blur", () => {
    const source = readFileSync(
      new URL("./Slider.tsx", import.meta.url),
      "utf8",
    );

    expect(source).toContain("onPointerUp={handleCommit}");
    expect(source).toContain("onKeyDown={handleKeyDown}");
    expect(source).toContain("onKeyUp={handleKeyUp}");
    expect(source).toContain("onBlur={handleCommit}");
    expect(source).toContain("onPointerCancel={handlePointerCancel}");
  });
});

describe("slider interaction", () => {
  test("persists one final value after rapid pointer movement", () => {
    const displayed: number[] = [];
    const committed: number[] = [];
    const interaction = createSliderInteraction(
      0.25,
      (value) => displayed.push(value),
      (value) => committed.push(value),
    );

    interaction.begin();
    interaction.update(0.35);
    interaction.update(0.55);
    interaction.update(0.75);

    expect(displayed).toEqual([0.35, 0.55, 0.75]);
    expect(committed).toEqual([]);

    interaction.finish();
    interaction.finish();
    expect(committed).toEqual([0.75]);
  });

  test("persists repeated keyboard input once on key release", () => {
    const committed: number[] = [];
    const interaction = createSliderInteraction(
      40,
      () => {},
      (value) => committed.push(value),
    );

    interaction.keyDown("ArrowRight");
    interaction.update(50);
    interaction.update(60);
    interaction.keyUp("ArrowRight");
    interaction.finish();

    expect(committed).toEqual([60]);
  });

  test("defers an external value during movement and restores it on cancel", () => {
    const displayed: number[] = [];
    const committed: number[] = [];
    const interaction = createSliderInteraction(
      40,
      (value) => displayed.push(value),
      (value) => committed.push(value),
    );

    interaction.begin();
    interaction.update(80);
    interaction.sync(60);
    interaction.cancel();

    expect(displayed).toEqual([80, 60]);
    expect(committed).toEqual([]);
    expect(interaction.value()).toBe(60);
  });
});
