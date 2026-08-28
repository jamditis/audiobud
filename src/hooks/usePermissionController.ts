import { useCallback, useEffect, useState } from "react";
import { platform } from "@tauri-apps/plugin-os";
import { commands } from "@/bindings";
import {
  checkMacOSAccessibilityPermission,
  checkMacOSMicrophonePermission,
  requestMacOSAccessibilityPermission,
  requestMacOSMicrophonePermission,
} from "@/lib/macos-permissions";
import {
  PermissionController,
  type PermissionBridge,
  type PermissionPlatform,
  type PermissionSnapshot,
} from "@/lib/permission-controller";

function currentPermissionPlatform(): PermissionPlatform {
  const currentPlatform = platform();
  if (currentPlatform === "macos" || currentPlatform === "windows") {
    return currentPlatform;
  }
  return "other";
}

const permissionBridge: PermissionBridge = {
  checkMacOSAccessibility: checkMacOSAccessibilityPermission,
  checkMacOSMicrophone: checkMacOSMicrophonePermission,
  async checkWindowsMicrophone() {
    const status = await commands.getWindowsMicrophonePermissionStatus();
    return !status.supported || status.overall_access !== "denied";
  },
  requestMacOSAccessibility: requestMacOSAccessibilityPermission,
  requestMacOSMicrophone: requestMacOSMicrophonePermission,
  async openWindowsMicrophoneSettings() {
    const result = await commands.openMicrophonePrivacySettings();
    if (result.status === "error") throw new Error(result.error);
  },
};

export interface PermissionControllerState extends PermissionSnapshot {
  check: () => Promise<PermissionSnapshot>;
  requestAccessibility: () => Promise<void>;
  requestMicrophone: () => Promise<void>;
}

export function usePermissionController(): PermissionControllerState {
  const [controller] = useState(
    () =>
      new PermissionController(currentPermissionPlatform(), permissionBridge),
  );
  const [snapshot, setSnapshot] = useState(controller.getSnapshot());

  useEffect(() => {
    const unsubscribe = controller.subscribe(setSnapshot);
    setSnapshot(controller.getSnapshot());
    const refreshAfterFocus = () => {
      void controller.check().catch((error) => {
        console.error("Failed to refresh system permissions:", error);
      });
    };

    window.addEventListener("focus", refreshAfterFocus);
    return () => {
      window.removeEventListener("focus", refreshAfterFocus);
      unsubscribe();
      controller.stop();
    };
  }, [controller]);

  const check = useCallback(() => controller.check(), [controller]);
  const requestAccessibility = useCallback(
    () => controller.requestAccessibility(),
    [controller],
  );
  const requestMicrophone = useCallback(
    () => controller.requestMicrophone(),
    [controller],
  );

  return {
    ...snapshot,
    check,
    requestAccessibility,
    requestMicrophone,
  };
}
