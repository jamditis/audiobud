import { describe, expect, test } from "bun:test";
import { claimOnboardingSelection } from "./onboardingSelection";

describe("claimOnboardingSelection", () => {
  test("claims a ready model only once across async model refreshes", () => {
    const guard = { modelId: null as string | null };

    expect(claimOnboardingSelection(guard, "small")).toBe(true);
    expect(claimOnboardingSelection(guard, "small")).toBe(false);
  });
});
