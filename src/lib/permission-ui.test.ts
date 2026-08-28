import { describe, expect, it } from "bun:test";
import { readFileSync } from "node:fs";

const app = readFileSync("src/App.tsx", "utf8");
const onboarding = readFileSync(
  "src/components/onboarding/AccessibilityOnboarding.tsx",
  "utf8",
);
const settingsWarning = readFileSync(
  "src/components/AccessibilityPermissions.tsx",
  "utf8",
);

describe("permission UI ownership", () => {
  it("renders request errors inline during onboarding", () => {
    expect(onboarding).toContain("permissions.error !== null");
    expect(onboarding).toContain(
      '"onboarding.permissions.errors.requestFailed"',
    );
    expect(onboarding).not.toContain('from "sonner"');
  });

  it("passes microphone recovery into the running-app warning", () => {
    expect(app).toContain(
      "onRequestMicrophone={permissions.requestMicrophone}",
    );
    expect(settingsWarning).toContain("permissionsNeedingAction(permissions)");
    expect(settingsWarning).toContain("onRequestMicrophone");
  });

  it("keeps permission state and polling out of both views", () => {
    for (const view of [onboarding, settingsWarning]) {
      expect(view).not.toContain("checkMacOSAccessibilityPermission");
      expect(view).not.toContain("checkMacOSMicrophonePermission");
      expect(view).not.toContain("setInterval");
      expect(view).not.toContain("useState");
    }
  });
});
