import { describe, expect, it } from "bun:test";
import {
  PermissionController,
  claimPermissionCompletion,
  permissionsNeedingAction,
  type PermissionBridge,
  type PermissionSnapshot,
} from "./permission-controller";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

async function settlePromises() {
  for (let index = 0; index < 8; index += 1) await Promise.resolve();
}

function bridge(overrides: Partial<PermissionBridge> = {}): PermissionBridge {
  return {
    checkMacOSAccessibility: async () => true,
    checkMacOSMicrophone: async () => true,
    checkWindowsMicrophone: async () => true,
    requestMacOSAccessibility: async () => {},
    requestMacOSMicrophone: async () => {},
    openWindowsMicrophoneSettings: async () => {},
    ...overrides,
  };
}

function manualScheduler() {
  const callbacks: Array<() => void> = [];

  return {
    schedule(callback: () => void) {
      callbacks.push(callback);
      return () => {
        const index = callbacks.indexOf(callback);
        if (index >= 0) callbacks.splice(index, 1);
      };
    },
    runNext() {
      const callback = callbacks.shift();
      if (!callback) throw new Error("No permission poll is scheduled");
      callback();
    },
    get size() {
      return callbacks.length;
    },
  };
}

function permissionPair(snapshot: PermissionSnapshot) {
  return {
    accessibility: snapshot.accessibility,
    microphone: snapshot.microphone,
    allGranted: snapshot.allGranted,
  };
}

describe("PermissionController", () => {
  for (const [accessibility, microphone, expected] of [
    [false, false, ["needed", "needed", false]],
    [false, true, ["needed", "granted", false]],
    [true, false, ["granted", "needed", false]],
    [true, true, ["granted", "granted", true]],
  ] as const) {
    it(`maps macOS accessibility=${accessibility} and microphone=${microphone}`, async () => {
      const controller = new PermissionController(
        "macos",
        bridge({
          checkMacOSAccessibility: async () => accessibility,
          checkMacOSMicrophone: async () => microphone,
        }),
      );

      const snapshot = await controller.check();

      expect(permissionPair(snapshot)).toEqual({
        accessibility: expected[0],
        microphone: expected[1],
        allGranted: expected[2],
      });
    });
  }

  it("requires only microphone permission on Windows", async () => {
    const controller = new PermissionController(
      "windows",
      bridge({ checkWindowsMicrophone: async () => false }),
    );

    expect(permissionPair(await controller.check())).toEqual({
      accessibility: "granted",
      microphone: "needed",
      allGranted: false,
    });
  });

  it("completes immediately on platforms without managed permissions", async () => {
    const controller = new PermissionController("other", bridge());

    expect(permissionPair(await controller.check())).toEqual({
      accessibility: "granted",
      microphone: "granted",
      allGranted: true,
    });
  });

  it("coalesces concurrent permission queries", async () => {
    const result = deferred<boolean>();
    let calls = 0;
    const controller = new PermissionController(
      "macos",
      bridge({
        checkMacOSAccessibility: () => {
          calls += 1;
          return result.promise;
        },
      }),
    );

    const first = controller.check();
    const second = controller.check();
    expect(calls).toBe(1);

    result.resolve(true);
    await Promise.all([first, second]);
    expect(calls).toBe(1);
  });

  it("coalesces concurrent accessibility requests and enters waiting immediately", async () => {
    const request = deferred<void>();
    let calls = 0;
    const controller = new PermissionController(
      "macos",
      bridge({
        requestMacOSAccessibility: () => {
          calls += 1;
          return request.promise;
        },
      }),
    );

    const first = controller.requestAccessibility();
    const second = controller.requestAccessibility();

    expect(calls).toBe(1);
    expect(controller.getSnapshot()).toMatchObject({
      accessibility: "waiting",
      allGranted: false,
    });

    request.resolve();
    await Promise.all([first, second]);
    expect(calls).toBe(1);
  });

  it("coalesces concurrent Windows microphone settings requests", async () => {
    const request = deferred<void>();
    let calls = 0;
    const controller = new PermissionController(
      "windows",
      bridge({
        openWindowsMicrophoneSettings: () => {
          calls += 1;
          return request.promise;
        },
      }),
    );

    const first = controller.requestMicrophone();
    const second = controller.requestMicrophone();

    expect(calls).toBe(1);
    expect(controller.getSnapshot()).toMatchObject({
      microphone: "waiting",
      allGranted: false,
    });

    request.resolve();
    await Promise.all([first, second]);
    expect(calls).toBe(1);
  });

  it("keeps a failed request visible and actionable", async () => {
    const controller = new PermissionController(
      "macos",
      bridge({
        requestMacOSMicrophone: async () => {
          throw new Error("request failed");
        },
      }),
    );

    await expect(controller.requestMicrophone()).rejects.toThrow(
      "request failed",
    );
    expect(controller.getSnapshot()).toMatchObject({
      microphone: "needed",
      allGranted: false,
      error: "request",
    });
    expect(permissionsNeedingAction(controller.getSnapshot())).toContain(
      "microphone",
    );
  });

  it("does not overlap slow polling queries", async () => {
    const scheduler = manualScheduler();
    const slowCheck = deferred<boolean>();
    let calls = 0;
    const controller = new PermissionController(
      "macos",
      bridge({
        checkMacOSAccessibility: () => {
          calls += 1;
          return slowCheck.promise;
        },
        checkMacOSMicrophone: async () => false,
      }),
      scheduler.schedule,
    );

    await controller.requestAccessibility();
    expect(scheduler.size).toBe(1);

    scheduler.runNext();
    await settlePromises();
    expect(calls).toBe(1);
    expect(scheduler.size).toBe(0);

    slowCheck.resolve(false);
    await settlePromises();
    expect(scheduler.size).toBe(1);
  });

  it("keeps a failed initial query in onboarding and permits retry", async () => {
    let shouldFail = true;
    const controller = new PermissionController(
      "macos",
      bridge({
        checkMacOSAccessibility: async () => {
          if (shouldFail) throw new Error("bridge unavailable");
          return true;
        },
      }),
    );

    await expect(controller.check()).rejects.toThrow("bridge unavailable");
    expect(controller.getSnapshot()).toMatchObject({
      accessibility: "needed",
      microphone: "needed",
      allGranted: false,
      error: "check",
    });

    shouldFail = false;
    expect(await controller.check()).toMatchObject({
      accessibility: "granted",
      microphone: "granted",
      allGranted: true,
      error: null,
    });
  });

  it("retries a denied accessibility request instead of only verifying", async () => {
    const scheduler = manualScheduler();
    let requests = 0;
    const controller = new PermissionController(
      "macos",
      bridge({
        checkMacOSAccessibility: async () => false,
        requestMacOSAccessibility: async () => {
          requests += 1;
        },
      }),
      scheduler.schedule,
    );

    await controller.requestAccessibility();
    scheduler.runNext();
    await settlePromises();
    await controller.requestAccessibility();

    expect(requests).toBe(2);
  });

  it("leaves the completed state before a permission request starts", async () => {
    const scheduler = manualScheduler();
    const controller = new PermissionController(
      "macos",
      bridge(),
      scheduler.schedule,
    );

    expect((await controller.check()).allGranted).toBe(true);
    await controller.requestAccessibility();

    expect(controller.getSnapshot()).toMatchObject({
      accessibility: "waiting",
      allGranted: false,
    });
  });

  it("returns a waiting permission to an actionable state after poll failures", async () => {
    const scheduler = manualScheduler();
    const controller = new PermissionController(
      "macos",
      bridge({
        checkMacOSAccessibility: async () => {
          throw new Error("bridge unavailable");
        },
      }),
      scheduler.schedule,
      2,
    );

    await controller.requestAccessibility();
    scheduler.runNext();
    await settlePromises();
    expect(scheduler.size).toBe(1);
    scheduler.runNext();
    await settlePromises();

    expect(controller.getSnapshot()).toMatchObject({
      accessibility: "needed",
      error: "check",
    });
    expect(scheduler.size).toBe(0);
  });

  it("detects permission revocation during a later refresh", async () => {
    let granted = true;
    const controller = new PermissionController(
      "macos",
      bridge({
        checkMacOSAccessibility: async () => granted,
      }),
    );

    expect((await controller.check()).allGranted).toBe(true);
    granted = false;
    expect(await controller.check()).toMatchObject({
      accessibility: "needed",
      allGranted: false,
    });
  });
});

describe("permissionsNeedingAction", () => {
  const granted: PermissionSnapshot = {
    platform: "macos",
    accessibility: "granted",
    microphone: "granted",
    allGranted: true,
    error: null,
  };

  it("shows microphone recovery after macOS revocation", () => {
    expect(
      permissionsNeedingAction({ ...granted, microphone: "needed" }),
    ).toEqual(["microphone"]);
  });

  it("shows microphone recovery after Windows revocation", () => {
    expect(
      permissionsNeedingAction({
        ...granted,
        platform: "windows",
        microphone: "needed",
      }),
    ).toEqual(["microphone"]);
  });

  it("shows both macOS recovery actions when both permissions are missing", () => {
    expect(
      permissionsNeedingAction({
        ...granted,
        accessibility: "needed",
        microphone: "needed",
      }),
    ).toEqual(["microphone", "accessibility"]);
  });
});

describe("claimPermissionCompletion", () => {
  it("allows one completion after all permissions are granted", () => {
    const guard = { completed: false };

    expect(claimPermissionCompletion(guard, false)).toBe(false);
    expect(claimPermissionCompletion(guard, true)).toBe(true);
    expect(claimPermissionCompletion(guard, true)).toBe(false);
  });
});
