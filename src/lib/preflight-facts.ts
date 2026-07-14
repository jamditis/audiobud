// Preflight facts adapter (#51).
//
// The decision core in preflight.ts is pure: given a typed SystemFacts it
// returns a report and one launchable boolean. This module is the other half the
// core's header names — the seam between a raw platform probe (a Tauri command
// that reads the OS version string, memory in bytes, free disk in bytes, and the
// detected accelerator) and the typed SystemFacts the core consumes. It does the
// normalization the core deliberately kept out of itself: deciding whether a
// Windows version string clears the Windows 10 floor, converting probed byte
// counts to the megabytes the thresholds use, and mapping an accelerator name to
// the core's Acceleration union.
//
// Fail-safe by construction, matching the core: every field is derived
// defensively, and a value the probe could not read (undefined or null) stays
// undefined on the facts. The core reads a missing field as `unknown` and
// surfaces it as a warning, never a block, so a machine that is actually fine but
// whose probe failed is never locked out. Keeping this parsing here — not in the
// core, not scattered in the Rust probe — means one tested place decides what a
// raw reading means, so the doc, the core, and the probe cannot drift on it.

import type { Acceleration, Platform, SystemFacts } from "./preflight";

/**
 * The raw, untyped readings a platform probe hands back. Every field beyond
 * `platform` is optional and may be null: a reading the probe could not take is
 * left null or undefined and drops out of the assembled facts, so the core sees
 * `unknown` rather than a wrong value. Byte fields are named for their unit so a
 * caller cannot pass kilobytes by mistake.
 */
export interface RawProbe {
  platform: Platform;
  /** CPU/OS architecture as the probe spells it ("x64", "x86_64", "AMD64", ...). */
  arch?: string | null;
  /** Total physical memory in bytes, as a probe such as sysinfo reports it. */
  totalRamBytes?: number | null;
  /** Free space in bytes on the volume AudioBud installs into. */
  freeDiskBytes?: number | null;
  /** The OS version string, e.g. "10.0.19045" on Windows. Windows-only. */
  osVersion?: string | null;
  /** Whether the WebView2 runtime is installed (#39). Windows-only. */
  webview2Present?: boolean | null;
  /** Whether the VC++ CRT and Vulkan loader are present (#36, #44). Windows-only. */
  runtimeDllsPresent?: boolean | null;
  /** The accelerator the app detected, in whatever spelling the probe used. */
  acceleration?: string | null;
}

const BYTES_PER_MB = 1024 * 1024;

/**
 * Convert a byte count to whole megabytes, or undefined when the reading is
 * missing or not a usable number. A negative or non-finite value (a probe error
 * surfaced as NaN or -1) reads as undefined, not as a zero that would trip a
 * false shortfall. Floors so the reported figure never overstates the memory a
 * machine actually has.
 */
export function mbFromBytes(
  bytes: number | null | undefined,
): number | undefined {
  if (bytes === null || bytes === undefined) return undefined;
  if (!Number.isFinite(bytes) || bytes < 0) return undefined;
  return Math.floor(bytes / BYTES_PER_MB);
}

/**
 * Whether a Windows version string names Windows 10 or newer, the documented
 * hard floor. Windows reports its version on the NT scheme, where both Windows 10
 * and 11 are major 10 ("10.0.<build>") and Windows 8.1/8/7 are major 6 — so the
 * floor is simply major >= 10, and the build number does not matter for this
 * gate. Returns undefined for a string it cannot parse (empty, or not starting
 * with a number), so an unreadable version fails safe to the core's `unknown`
 * warning rather than a false block. The core owns the pass/fail policy; this
 * only turns the raw string into the boolean verdict the core asked the adapter
 * to supply.
 */
export function parseWindowsVersionSupported(
  osVersion: string | null | undefined,
): boolean | undefined {
  if (osVersion === null || osVersion === undefined) return undefined;
  const match = osVersion.trim().match(/^(\d+)/);
  if (!match) return undefined;
  const major = Number(match[1]);
  if (!Number.isInteger(major)) return undefined;
  return major >= 10;
}

// Keep in sync with the Acceleration union in preflight.ts. A subset still
// type-checks here, so if a new accelerator is added to the union but missed
// from this list it would silently normalize to undefined and surface as a
// spurious "no accelerator" warning on a machine that has one.
const ACCELERATION_VALUES: readonly Acceleration[] = [
  "vulkan",
  "directml",
  "metal",
  "openblas",
  "cpu",
  "none",
];

// Documented aliases for accelerator names a probe may use that are not the
// core's own spelling. The Apple path is CoreML, which runs on Metal (see the
// acceleration settings help text), so it maps to metal. Anything unrecognized
// stays undefined so the core treats it as unknown, never as a wrong value.
const ACCELERATION_ALIASES: Readonly<Record<string, Acceleration>> = {
  coreml: "metal",
};

/**
 * Map a probed accelerator name to the core's Acceleration union, or undefined
 * when it is missing or unrecognized. Matching is case- and whitespace-
 * insensitive so "DirectML" and " vulkan " normalize cleanly.
 */
export function normalizeAcceleration(
  raw: string | null | undefined,
): Acceleration | undefined {
  if (raw === null || raw === undefined) return undefined;
  const key = raw.trim().toLowerCase();
  if (key === "") return undefined;
  if ((ACCELERATION_VALUES as readonly string[]).includes(key)) {
    return key as Acceleration;
  }
  return ACCELERATION_ALIASES[key];
}

/**
 * Assemble the typed SystemFacts the decision core consumes from a raw probe.
 * The Windows-only fields (windowsVersionSupported, webview2Present,
 * runtimeDllsPresent) are derived only on Windows; on other platforms they stay
 * undefined and the core's platform-specific check list skips them. Any field the
 * probe could not read stays undefined and evaluates to `unknown` in the core,
 * never to a failing value — so this adapter can never be the reason a working
 * machine is blocked.
 */
export function buildSystemFacts(raw: RawProbe): SystemFacts {
  const facts: SystemFacts = { platform: raw.platform };

  if (raw.arch !== null && raw.arch !== undefined) {
    // A blank or whitespace-only arch means the probe failed to read it. On
    // Windows the core hard-blocks any defined non-x64 arch, so writing a blank
    // value would falsely fail a machine whose architecture simply could not be
    // determined; omit it like blank OS versions and accelerators so it reads as
    // `unknown`.
    const arch = raw.arch.trim();
    if (arch !== "") {
      facts.arch = arch;
    }
  }

  const totalRamMb = mbFromBytes(raw.totalRamBytes);
  if (totalRamMb !== undefined) {
    facts.totalRamMb = totalRamMb;
  }

  const freeDiskMb = mbFromBytes(raw.freeDiskBytes);
  if (freeDiskMb !== undefined) {
    facts.freeDiskMb = freeDiskMb;
  }

  const acceleration = normalizeAcceleration(raw.acceleration);
  if (acceleration !== undefined) {
    facts.acceleration = acceleration;
  }

  if (raw.platform === "windows") {
    const windowsVersionSupported = parseWindowsVersionSupported(raw.osVersion);
    if (windowsVersionSupported !== undefined) {
      facts.windowsVersionSupported = windowsVersionSupported;
    }
    if (raw.webview2Present !== null && raw.webview2Present !== undefined) {
      facts.webview2Present = raw.webview2Present;
    }
    if (
      raw.runtimeDllsPresent !== null &&
      raw.runtimeDllsPresent !== undefined
    ) {
      facts.runtimeDllsPresent = raw.runtimeDllsPresent;
    }
  }

  return facts;
}
