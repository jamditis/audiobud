import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { claimOnboardingSelection } from "./onboardingSelection";

const onboarding = readFileSync(
  "src/components/onboarding/Onboarding.tsx",
  "utf8",
);

describe("claimOnboardingSelection", () => {
  test("claims a ready model only once across async model refreshes", () => {
    const guard = { modelId: null as string | null };

    expect(claimOnboardingSelection(guard, "small")).toBe(true);
    expect(claimOnboardingSelection(guard, "small")).toBe(false);
  });

  test("a cancel unlocks selection without clearing a retry during refresh", () => {
    const watcherStart = onboarding.indexOf("useEffect(() => {");
    const watcherEnd = onboarding.indexOf(
      "const handleDownloadModel",
      watcherStart,
    );
    const cancelStart = onboarding.indexOf("const handleCancelDownload");
    const cancelEnd = onboarding.indexOf("const getModelStatus", cancelStart);
    const watcher = onboarding.slice(watcherStart, watcherEnd);
    const cancelHandler = onboarding.slice(cancelStart, cancelEnd);

    expect(watcherStart).toBeGreaterThan(-1);
    expect(watcherEnd).toBeGreaterThan(watcherStart);
    expect(cancelStart).toBeGreaterThan(-1);
    expect(cancelEnd).toBeGreaterThan(cancelStart);
    expect(watcher).not.toContain(
      "else if (!model?.is_downloaded && !inFlight)",
    );
    expect(cancelHandler).toContain("if (await cancelDownload(modelId))");
    expect(cancelHandler).toContain("setSelectedModelId(null)");
  });
});
