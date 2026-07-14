import { describe, it, expect } from "bun:test";
import {
  buildSystemFacts,
  mbFromBytes,
  normalizeAcceleration,
  parseWindowsVersionSupported,
  type RawProbe,
} from "./preflight-facts";
import {
  evaluatePreflight,
  MIN_RAM_MB,
  RECOMMENDED_RAM_MB,
  MIN_FREE_DISK_MB,
} from "./preflight";

// The adapter turns a raw platform probe into the typed SystemFacts the decision
// core consumes. What matters is not line coverage but that it preserves the
// core's fail-safe contract end to end: a reading the probe could not take must
// become `undefined` (which the core treats as `unknown`, a warning) and never a
// value that would falsely block a machine that is actually fine.

describe("mbFromBytes", () => {
  it("converts bytes to whole megabytes, flooring", () => {
    expect(mbFromBytes(8 * 1024 * 1024 * 1024)).toBe(8192);
    // 4.5 MB floors to 4, never rounding up to overstate available memory.
    expect(mbFromBytes(4.5 * 1024 * 1024)).toBe(4);
  });

  it("reads a missing or unusable value as undefined, not zero", () => {
    expect(mbFromBytes(undefined)).toBeUndefined();
    expect(mbFromBytes(null)).toBeUndefined();
    expect(mbFromBytes(-1)).toBeUndefined();
    expect(mbFromBytes(Number.NaN)).toBeUndefined();
    expect(mbFromBytes(Number.POSITIVE_INFINITY)).toBeUndefined();
  });
});

describe("parseWindowsVersionSupported", () => {
  it("accepts Windows 10 and 11 (NT major 10)", () => {
    expect(parseWindowsVersionSupported("10.0.19045")).toBe(true); // Windows 10
    expect(parseWindowsVersionSupported("10.0.22631")).toBe(true); // Windows 11
    expect(parseWindowsVersionSupported(" 10.0.19045 ")).toBe(true); // trimmed
  });

  it("rejects Windows 8.1 and older (NT major 6 or 5)", () => {
    expect(parseWindowsVersionSupported("6.3.9600")).toBe(false); // Windows 8.1
    expect(parseWindowsVersionSupported("6.1.7601")).toBe(false); // Windows 7
    expect(parseWindowsVersionSupported("5.1.2600")).toBe(false); // Windows XP
  });

  it("reads an unparseable or missing version as undefined (fail-safe)", () => {
    expect(parseWindowsVersionSupported(undefined)).toBeUndefined();
    expect(parseWindowsVersionSupported(null)).toBeUndefined();
    expect(parseWindowsVersionSupported("")).toBeUndefined();
    expect(parseWindowsVersionSupported("unknown")).toBeUndefined();
  });
});

describe("normalizeAcceleration", () => {
  it("passes through the core's accelerator spellings, case-insensitively", () => {
    expect(normalizeAcceleration("directml")).toBe("directml");
    expect(normalizeAcceleration("DirectML")).toBe("directml");
    expect(normalizeAcceleration(" vulkan ")).toBe("vulkan");
    expect(normalizeAcceleration("none")).toBe("none");
  });

  it("maps the CoreML alias to the Metal path", () => {
    expect(normalizeAcceleration("coreml")).toBe("metal");
  });

  it("reads a missing or unrecognized accelerator as undefined", () => {
    expect(normalizeAcceleration(undefined)).toBeUndefined();
    expect(normalizeAcceleration(null)).toBeUndefined();
    expect(normalizeAcceleration("")).toBeUndefined();
    expect(normalizeAcceleration("cuda")).toBeUndefined(); // not a bundled path
  });
});

describe("buildSystemFacts assembles typed facts from a raw probe", () => {
  it("converts and carries the readings a probe supplied", () => {
    const facts = buildSystemFacts({
      platform: "windows",
      arch: "x86_64",
      totalRamBytes: 16 * 1024 * 1024 * 1024,
      freeDiskBytes: 50 * 1024 * 1024 * 1024,
      osVersion: "10.0.22631",
      webview2Present: true,
      runtimeDllsPresent: true,
      acceleration: "DirectML",
    });
    expect(facts).toEqual({
      platform: "windows",
      arch: "x86_64",
      totalRamMb: 16384,
      freeDiskMb: 51200,
      windowsVersionSupported: true,
      webview2Present: true,
      runtimeDllsPresent: true,
      acceleration: "directml",
    });
  });

  it("drops fields the probe could not read rather than inventing values", () => {
    const facts = buildSystemFacts({
      platform: "windows",
      arch: null,
      totalRamBytes: null,
      osVersion: null,
      webview2Present: null,
    });
    // Only the platform survives; every unread field is absent, so the core
    // reads each as `unknown`.
    expect(facts).toEqual({ platform: "windows" });
  });

  it("omits a blank arch so an unreadable probe cannot hard-block a good machine", () => {
    // An empty or whitespace-only arch means the probe failed to read it. On
    // Windows the core hard-blocks any defined non-x64 arch, so a blank value
    // written through would falsely fail a machine whose arch could not be
    // determined; it must stay absent and read as `unknown` instead.
    expect(
      buildSystemFacts({ platform: "windows", arch: "" }).arch,
    ).toBeUndefined();
    expect(
      buildSystemFacts({ platform: "windows", arch: "   " }).arch,
    ).toBeUndefined();
    // A real reading is still carried, trimmed of stray probe whitespace.
    expect(buildSystemFacts({ platform: "linux", arch: " arm64 " }).arch).toBe(
      "arm64",
    );
  });

  it("derives Windows-only fields only on Windows", () => {
    const raw: Omit<RawProbe, "platform"> = {
      osVersion: "6.1.7601",
      webview2Present: false,
      runtimeDllsPresent: false,
    };
    const mac = buildSystemFacts({ platform: "macos", ...raw });
    expect(mac.windowsVersionSupported).toBeUndefined();
    expect(mac.webview2Present).toBeUndefined();
    expect(mac.runtimeDllsPresent).toBeUndefined();
    const win = buildSystemFacts({ platform: "windows", ...raw });
    expect(win.windowsVersionSupported).toBe(false);
    expect(win.webview2Present).toBe(false);
  });
});

describe("adapter feeds the decision core and preserves its fail-safe contract", () => {
  it("blocks a real under-spec Windows machine end to end", () => {
    // A raw Windows 7 probe: the version string alone must drive a hard block.
    const report = evaluatePreflight(
      buildSystemFacts({
        platform: "windows",
        arch: "AMD64",
        totalRamBytes: RECOMMENDED_RAM_MB * 1024 * 1024,
        freeDiskBytes: MIN_FREE_DISK_MB * 2 * 1024 * 1024,
        osVersion: "6.1.7601",
        webview2Present: true,
        runtimeDllsPresent: true,
        acceleration: "vulkan",
      }),
    );
    expect(report.launchable).toBe(false);
    expect(report.blocking.some((r) => r.id === "windows-version")).toBe(true);
  });

  it("never blocks when the version probe failed, only warns", () => {
    // Everything else is fine but the OS version could not be read. A failed
    // probe must not lock out a machine that is actually on Windows 10+.
    const report = evaluatePreflight(
      buildSystemFacts({
        platform: "windows",
        arch: "x64",
        totalRamBytes: RECOMMENDED_RAM_MB * 1024 * 1024,
        freeDiskBytes: MIN_FREE_DISK_MB * 2 * 1024 * 1024,
        osVersion: null,
        webview2Present: true,
        runtimeDllsPresent: true,
        acceleration: "directml",
      }),
    );
    expect(report.launchable).toBe(true);
    expect(report.warnings.some((r) => r.id === "windows-version")).toBe(true);
  });

  it("never blocks when the arch probe read blank, only warns", () => {
    // The sibling of the version case, and the failure this fix exists to
    // prevent: on Windows the arch check is a hard gate, so a probe that
    // returned a blank arch must warn (unknown), never block a good machine.
    const report = evaluatePreflight(
      buildSystemFacts({
        platform: "windows",
        arch: "   ",
        totalRamBytes: RECOMMENDED_RAM_MB * 1024 * 1024,
        freeDiskBytes: MIN_FREE_DISK_MB * 2 * 1024 * 1024,
        osVersion: "10.0.19045",
        webview2Present: true,
        runtimeDllsPresent: true,
        acceleration: "directml",
      }),
    );
    expect(report.launchable).toBe(true);
    expect(report.blocking.some((r) => r.id === "arch")).toBe(false);
    expect(report.warnings.some((r) => r.id === "arch")).toBe(true);
  });

  it("passes a fully-good Windows machine with no blocks or warnings", () => {
    const report = evaluatePreflight(
      buildSystemFacts({
        platform: "windows",
        arch: "x64",
        totalRamBytes: RECOMMENDED_RAM_MB * 1024 * 1024,
        freeDiskBytes: MIN_FREE_DISK_MB * 2 * 1024 * 1024,
        osVersion: "10.0.19045",
        webview2Present: true,
        runtimeDllsPresent: true,
        acceleration: "directml",
      }),
    );
    expect(report.launchable).toBe(true);
    expect(report.blocking).toHaveLength(0);
    expect(report.warnings).toHaveLength(0);
  });

  it("blocks an ARM machine through the passed-through arch", () => {
    const report = evaluatePreflight(
      buildSystemFacts({
        platform: "windows",
        arch: "arm64",
        osVersion: "10.0.22631",
      }),
    );
    expect(report.launchable).toBe(false);
    expect(report.blocking.some((r) => r.id === "arch")).toBe(true);
  });

  it("never blocks the validated platform when the probe read nothing at all", () => {
    // The worst case behind the issue's "don't scare off users whose machines
    // are fine" open question: a probe that returned only the platform (not
    // wired yet, or crashed). Every hard check reads `unknown`, none `missing`,
    // so launch stays allowed — the adapter can never lock out a working machine.
    const report = evaluatePreflight(buildSystemFacts({ platform: "windows" }));
    expect(report.launchable).toBe(true);
    expect(report.blocking).toHaveLength(0);
  });

  it("runs only soft checks on an unvalidated platform, so low RAM warns not blocks", () => {
    const report = evaluatePreflight(
      buildSystemFacts({
        platform: "macos",
        arch: "arm64",
        totalRamBytes: (MIN_RAM_MB - 1024) * 1024 * 1024,
        acceleration: "coreml",
      }),
    );
    expect(report.launchable).toBe(true);
    expect(report.warnings.some((r) => r.id === "ram")).toBe(true);
    // The CoreML alias resolved to the Metal path, so acceleration is not a warning.
    expect(report.warnings.some((r) => r.id === "acceleration")).toBe(false);
  });
});
