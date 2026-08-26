import { describe, expect, it } from "bun:test";
import type { OutputTargetLockEvent } from "@/bindings";
import type { LockSnapshot } from "@/lib/output-target-indicator";
import { subscribeToOutputTargetLock, toSnapshot } from "./useOutputTargetLock";

/** A promise plus its resolve function, for controlling settlement order in tests. */
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}

// The backend sends Option<String> fields, which specta maps to `T | null`;
// the indicator core (output-target-indicator.ts) uses the optional-property
// convention (`T | undefined`) instead. toSnapshot is the one place that
// reconciles the two, so it is the one place this needs proving (#255).

describe("toSnapshot", () => {
  it("maps an unlocked event with no app/title fields", () => {
    expect(toSnapshot({ kind: "unlocked" })).toEqual({ kind: "unlocked" });
  });

  it("maps null app/title to undefined for a locked snapshot", () => {
    expect(toSnapshot({ kind: "locked", app: null, title: null })).toEqual({
      kind: "locked",
      app: undefined,
      title: undefined,
    });
  });

  it("passes real app/title strings through unchanged", () => {
    expect(
      toSnapshot({ kind: "locked", app: "Terminal", title: "zsh" }),
    ).toEqual({ kind: "locked", app: "Terminal", title: "zsh" });
  });

  it("maps a lost snapshot the same way as locked", () => {
    expect(toSnapshot({ kind: "lost", app: "Terminal", title: null })).toEqual({
      kind: "lost",
      app: "Terminal",
      title: undefined,
    });
  });
});

// subscribeToOutputTargetLock owns the ordering guarantee between the
// initial snapshot command and the live event stream (#266 review): both
// cross the Tauri IPC bridge asynchronously, so firing them without care
// races. These tests control settlement order directly with `deferred()`
// rather than relying on real timing.

describe("subscribeToOutputTargetLock", () => {
  it("subscribes before querying", () => {
    const calls: string[] = [];
    const query = () => {
      calls.push("query");
      return Promise.resolve<OutputTargetLockEvent>({ kind: "unlocked" });
    };
    const subscribe = (_onEvent: (event: OutputTargetLockEvent) => void) => {
      calls.push("subscribe");
      return Promise.resolve(() => {});
    };

    subscribeToOutputTargetLock(query, subscribe, () => {});

    // subscribe() is called synchronously within the same tick; query() is
    // deferred until subscribe's promise resolves, so it cannot appear first.
    expect(calls).toEqual(["subscribe"]);
  });

  it("applies the initial snapshot when no event arrives first", async () => {
    const snapshots: LockSnapshot[] = [];
    const query = () =>
      Promise.resolve<OutputTargetLockEvent>({
        kind: "locked",
        app: "Terminal",
        title: null,
      });
    const subscribe = (_onEvent: (event: OutputTargetLockEvent) => void) =>
      Promise.resolve(() => {});

    subscribeToOutputTargetLock(query, subscribe, (s) => snapshots.push(s));
    // Let the subscribe -> query -> onSnapshot microtask chain settle.
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();

    expect(snapshots).toEqual([
      { kind: "locked", app: "Terminal", title: undefined },
    ]);
  });

  it("discards a slow initial read that resolves after a newer event", async () => {
    // The scenario the finding described: query() is in flight (e.g. the
    // window just got locked and unlocked again before the initial read
    // came back), and a fresher event arrives first. The stale query
    // response must not clobber it.
    const snapshots: LockSnapshot[] = [];
    const queryResult = deferred<OutputTargetLockEvent>();
    const query = () => queryResult.promise;

    let deliverEvent!: (event: OutputTargetLockEvent) => void;
    const subscribe = (onEvent: (event: OutputTargetLockEvent) => void) => {
      deliverEvent = onEvent;
      return Promise.resolve(() => {});
    };

    subscribeToOutputTargetLock(query, subscribe, (s) => snapshots.push(s));
    // Let subscribe's promise resolve so query() actually fires.
    await Promise.resolve();
    await Promise.resolve();

    // A newer event beats the still-pending initial read.
    deliverEvent({ kind: "locked", app: "Editor", title: null });
    // The stale command response arrives after it.
    queryResult.resolve({ kind: "unlocked" });
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();

    expect(snapshots).toEqual([
      { kind: "locked", app: "Editor", title: undefined },
    ]);
  });

  it("stops applying snapshots after cleanup runs", async () => {
    const snapshots: LockSnapshot[] = [];
    const queryResult = deferred<OutputTargetLockEvent>();
    const query = () => queryResult.promise;
    const subscribe = (_onEvent: (event: OutputTargetLockEvent) => void) =>
      Promise.resolve(() => {});

    const cleanup = subscribeToOutputTargetLock(query, subscribe, (s) =>
      snapshots.push(s),
    );
    await Promise.resolve();
    await Promise.resolve();
    cleanup();
    queryResult.resolve({ kind: "unlocked" });
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();

    expect(snapshots).toEqual([]);
  });

  it("never throws when the bridge is unavailable (subscribe rejects)", async () => {
    const query = () =>
      Promise.resolve<OutputTargetLockEvent>({ kind: "unlocked" });
    const subscribe = (_onEvent: (event: OutputTargetLockEvent) => void) =>
      Promise.reject(new Error("no Tauri bridge"));

    const cleanup = subscribeToOutputTargetLock(query, subscribe, () => {});
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();

    // Cleanup itself must not throw either, even though the listen promise
    // it awaits was the one that rejected.
    expect(() => cleanup()).not.toThrow();
  });
});
