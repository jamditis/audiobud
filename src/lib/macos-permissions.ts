type MacOSPermissionsModule =
  typeof import("tauri-plugin-macos-permissions-api");

let modulePromise: Promise<MacOSPermissionsModule> | null = null;

function loadMacOSPermissions(): Promise<MacOSPermissionsModule> {
  modulePromise ??= import("tauri-plugin-macos-permissions-api");
  return modulePromise;
}

export async function checkMacOSAccessibilityPermission(): Promise<boolean> {
  const permissions = await loadMacOSPermissions();
  return permissions.checkAccessibilityPermission();
}

export async function requestMacOSAccessibilityPermission(): Promise<void> {
  const permissions = await loadMacOSPermissions();
  await permissions.requestAccessibilityPermission();
}

export async function checkMacOSMicrophonePermission(): Promise<boolean> {
  const permissions = await loadMacOSPermissions();
  return permissions.checkMicrophonePermission();
}

export async function requestMacOSMicrophonePermission(): Promise<void> {
  const permissions = await loadMacOSPermissions();
  await permissions.requestMicrophonePermission();
}
