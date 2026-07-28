import { describe, expect, it } from "bun:test";
import { onMediaQueryChange } from "./useMicLevel";

// The reduce-motion subscription has to work on both MediaQueryList APIs, because
// tauri.conf.json still ships a 10.15 minimum and Catalina's WKWebView implements
// matchMedia without addEventListener. That branch cannot execute in a modern
// engine, so the only way to prove it is to hand the function a query object
// shaped like the old one.

type Handler = (event: MediaQueryListEvent) => void;

interface FakeQuery {
  /** The MediaQueryList stand-in, with only one of the two listener APIs. */
  query: MediaQueryList;
  /** Listener calls in order, so a silent no-op cannot pass. */
  calls: string[];
  /** Fire a change the way the browser would, or throw if nothing is listening. */
  emit: (matches: boolean) => void;
  isSubscribed: () => boolean;
}

/** A Safari 14+ MediaQueryList: the modern listener API only. */
function modernQuery(): FakeQuery {
  const calls: string[] = [];
  let registered: Handler | null = null;
  const query = {
    matches: false,
    addEventListener(type: string, handler: Handler) {
      calls.push(`add:${type}`);
      registered = handler;
    },
    removeEventListener(type: string, handler: Handler) {
      calls.push(`remove:${type}`);
      if (registered === handler) registered = null;
    },
  };
  return {
    query: query as unknown as MediaQueryList,
    calls,
    emit: (matches) => {
      if (!registered) throw new Error("no listener registered");
      registered({ matches } as MediaQueryListEvent);
    },
    isSubscribed: () => registered !== null,
  };
}

/** A Catalina-era MediaQueryList: addListener/removeListener and nothing else. */
function legacyQuery(): FakeQuery {
  const calls: string[] = [];
  let registered: Handler | null = null;
  const query = {
    matches: false,
    addListener(handler: Handler) {
      calls.push("addListener");
      registered = handler;
    },
    removeListener(handler: Handler) {
      calls.push("removeListener");
      if (registered === handler) registered = null;
    },
  };
  return {
    query: query as unknown as MediaQueryList,
    calls,
    emit: (matches) => {
      if (!registered) throw new Error("no listener registered");
      registered({ matches } as MediaQueryListEvent);
    },
    isSubscribed: () => registered !== null,
  };
}

const noop: Handler = () => {};

describe("onMediaQueryChange", () => {
  it("uses the modern listener API when it is available", () => {
    const { query, calls, isSubscribed } = modernQuery();
    const unsubscribe = onMediaQueryChange(query, noop);
    expect(calls).toEqual(["add:change"]);
    expect(isSubscribed()).toBe(true);
    unsubscribe();
    expect(calls).toEqual(["add:change", "remove:change"]);
    expect(isSubscribed()).toBe(false);
  });

  it("falls back to addListener rather than throwing on an old WKWebView", () => {
    // The regression this exists for: addEventListener is absent there, so calling
    // it throws during mount, and because the hook mounts in both LiveFrog and
    // RecordingOverlay the throw takes the whole UI down over an animation
    // preference.
    const { query, calls, isSubscribed } = legacyQuery();
    const unsubscribe = onMediaQueryChange(query, noop);
    expect(calls).toEqual(["addListener"]);
    expect(isSubscribed()).toBe(true);
    unsubscribe();
    expect(calls).toEqual(["addListener", "removeListener"]);
    expect(isSubscribed()).toBe(false);
  });

  it("delivers changes on both APIs, and stops after unsubscribe", () => {
    // Registering without receiving would be a silent no-op — on the legacy path
    // just as broken as the throw, only quieter.
    for (const build of [modernQuery, legacyQuery]) {
      const fake = build();
      const seen: boolean[] = [];
      const unsubscribe = onMediaQueryChange(fake.query, (event) =>
        seen.push(event.matches),
      );

      fake.emit(true);
      fake.emit(false);
      expect(seen).toEqual([true, false]);

      unsubscribe();
      expect(() => fake.emit(true)).toThrow("no listener registered");
      expect(seen).toEqual([true, false]);
    }
  });
});
