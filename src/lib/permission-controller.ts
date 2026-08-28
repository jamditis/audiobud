export type PermissionPlatform = "macos" | "windows" | "other";
export type PermissionStatus = "checking" | "needed" | "waiting" | "granted";
export type PermissionError = "check" | "request" | null;
export type ManagedPermission = "microphone" | "accessibility";

export interface PermissionSnapshot {
  platform: PermissionPlatform;
  accessibility: PermissionStatus;
  microphone: PermissionStatus;
  allGranted: boolean;
  error: PermissionError;
}

export interface PermissionBridge {
  checkMacOSAccessibility: () => Promise<boolean>;
  checkMacOSMicrophone: () => Promise<boolean>;
  checkWindowsMicrophone: () => Promise<boolean>;
  requestMacOSAccessibility: () => Promise<void>;
  requestMacOSMicrophone: () => Promise<void>;
  openWindowsMicrophoneSettings: () => Promise<void>;
}

type CancelScheduledPoll = () => void;
type SchedulePoll = (callback: () => void) => CancelScheduledPoll;
type SnapshotListener = (snapshot: PermissionSnapshot) => void;

export interface PermissionCompletionGuard {
  completed: boolean;
}

const defaultSchedulePoll: SchedulePoll = (callback) => {
  const timer = setTimeout(callback, 1000);
  return () => clearTimeout(timer);
};

function initialSnapshot(platform: PermissionPlatform): PermissionSnapshot {
  if (platform === "macos") {
    return {
      platform,
      accessibility: "checking",
      microphone: "checking",
      allGranted: false,
      error: null,
    };
  }

  if (platform === "windows") {
    return {
      platform,
      accessibility: "granted",
      microphone: "checking",
      allGranted: false,
      error: null,
    };
  }

  return {
    platform,
    accessibility: "granted",
    microphone: "granted",
    allGranted: true,
    error: null,
  };
}

function statusFor(granted: boolean): PermissionStatus {
  return granted ? "granted" : "needed";
}

export function permissionsNeedingAction(
  snapshot: PermissionSnapshot,
): ManagedPermission[] {
  if (snapshot.platform === "other") return [];

  const permissions: ManagedPermission[] = [];
  if (snapshot.microphone !== "granted") permissions.push("microphone");
  if (snapshot.platform === "macos" && snapshot.accessibility !== "granted") {
    permissions.push("accessibility");
  }
  return permissions;
}

export function claimPermissionCompletion(
  guard: PermissionCompletionGuard,
  allGranted: boolean,
): boolean {
  if (!allGranted || guard.completed) return false;

  guard.completed = true;
  return true;
}

export class PermissionController {
  private snapshot: PermissionSnapshot;
  private readonly listeners = new Set<SnapshotListener>();
  private checkInFlight: Promise<PermissionSnapshot> | null = null;
  private accessibilityRequestInFlight: Promise<void> | null = null;
  private microphoneRequestInFlight: Promise<void> | null = null;
  private cancelScheduledPoll: CancelScheduledPoll | null = null;
  private isPolling = false;
  private pollingErrors = 0;

  constructor(
    private readonly platform: PermissionPlatform,
    private readonly bridge: PermissionBridge,
    private readonly schedulePoll: SchedulePoll = defaultSchedulePoll,
    private readonly maxPollingErrors = 3,
  ) {
    this.snapshot = initialSnapshot(platform);
  }

  getSnapshot(): PermissionSnapshot {
    return this.snapshot;
  }

  subscribe(listener: SnapshotListener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  check(): Promise<PermissionSnapshot> {
    if (this.checkInFlight) return this.checkInFlight;

    const check = this.performCheck().finally(() => {
      if (this.checkInFlight === check) this.checkInFlight = null;
    });
    this.checkInFlight = check;
    return check;
  }

  requestAccessibility(): Promise<void> {
    if (this.platform !== "macos") return Promise.resolve();
    if (this.accessibilityRequestInFlight) {
      return this.accessibilityRequestInFlight;
    }

    this.update({
      accessibility: "waiting",
      allGranted: false,
      error: null,
    });
    const request = this.performAccessibilityRequest().finally(() => {
      if (this.accessibilityRequestInFlight === request) {
        this.accessibilityRequestInFlight = null;
      }
    });
    this.accessibilityRequestInFlight = request;
    return request;
  }

  private async performAccessibilityRequest(): Promise<void> {
    try {
      await this.bridge.requestMacOSAccessibility();
      this.startPolling();
    } catch (error) {
      this.update({
        accessibility: "needed",
        allGranted: false,
        error: "request",
      });
      throw error;
    }
  }

  requestMicrophone(): Promise<void> {
    if (this.platform === "other") return Promise.resolve();
    if (this.microphoneRequestInFlight) {
      return this.microphoneRequestInFlight;
    }

    this.update({ microphone: "waiting", allGranted: false, error: null });
    const request = this.performMicrophoneRequest().finally(() => {
      if (this.microphoneRequestInFlight === request) {
        this.microphoneRequestInFlight = null;
      }
    });
    this.microphoneRequestInFlight = request;
    return request;
  }

  private async performMicrophoneRequest(): Promise<void> {
    try {
      if (this.platform === "macos") {
        await this.bridge.requestMacOSMicrophone();
      } else if (this.platform === "windows") {
        await this.bridge.openWindowsMicrophoneSettings();
      }

      this.startPolling();
    } catch (error) {
      this.update({
        microphone: "needed",
        allGranted: false,
        error: "request",
      });
      throw error;
    }
  }

  stop(): void {
    this.stopPolling();
  }

  private async performCheck(): Promise<PermissionSnapshot> {
    try {
      let accessibilityGranted = true;
      let microphoneGranted = true;

      if (this.platform === "macos") {
        [accessibilityGranted, microphoneGranted] = await Promise.all([
          this.bridge.checkMacOSAccessibility(),
          this.bridge.checkMacOSMicrophone(),
        ]);
      } else if (this.platform === "windows") {
        microphoneGranted = await this.bridge.checkWindowsMicrophone();
      }

      const allGranted = accessibilityGranted && microphoneGranted;
      this.update({
        accessibility: statusFor(accessibilityGranted),
        microphone: statusFor(microphoneGranted),
        allGranted,
        error: null,
      });

      if (allGranted) this.stopPolling();
      return this.snapshot;
    } catch (error) {
      this.update({
        accessibility:
          this.snapshot.accessibility === "granted" ? "granted" : "needed",
        microphone:
          this.snapshot.microphone === "granted" ? "granted" : "needed",
        allGranted: false,
        error: "check",
      });
      throw error;
    }
  }

  private startPolling(): void {
    if (this.isPolling) return;

    this.isPolling = true;
    this.pollingErrors = 0;
    this.scheduleNextPoll();
  }

  private scheduleNextPoll(): void {
    if (!this.isPolling || this.cancelScheduledPoll) return;

    this.cancelScheduledPoll = this.schedulePoll(() => {
      this.cancelScheduledPoll = null;
      void this.pollOnce();
    });
  }

  private async pollOnce(): Promise<void> {
    if (!this.isPolling) return;

    try {
      const snapshot = await this.check();
      this.pollingErrors = 0;
      if (snapshot.allGranted) return;
    } catch {
      this.pollingErrors += 1;
      if (this.pollingErrors >= this.maxPollingErrors) {
        this.stopPolling();
        return;
      }
    }

    this.scheduleNextPoll();
  }

  private stopPolling(): void {
    this.isPolling = false;
    this.pollingErrors = 0;
    this.cancelScheduledPoll?.();
    this.cancelScheduledPoll = null;
  }

  private update(update: Partial<PermissionSnapshot>): void {
    this.snapshot = { ...this.snapshot, ...update };
    for (const listener of this.listeners) listener(this.snapshot);
  }
}
