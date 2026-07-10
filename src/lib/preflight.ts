// Preflight system-requirements check (#51).
//
// AudioBud bundles native STT engines with platform-specific acceleration and
// runtime dependencies, so an under-spec machine fails opaquely: a missing
// WebView2 runtime or VC++ DLL means the app won't start or transcription
// silently fails, and too little RAM can OOM mid-transcription. This module is
// the framework-free decision core of the fix: given a set of probed system
// facts, it returns a structured report of which requirements pass, which are
// missing, and which are merely degraded — each with a plain-language message
// and a fix — plus one boolean the caller gates launch on.
//
// It is pure and imports nothing: the actual probing (Tauri commands that read
// the OS version, RAM, free disk, WebView2 presence, and the detected
// accelerator) is the platform adapter slice, and the first-run panel is the UI
// slice. Both feed this core and render its report, so the policy of what counts
// as a hard vs. soft shortfall lives in one tested place, not scattered across a
// Rust probe and a React panel. Documented minimums: SYSTEM_REQUIREMENTS.md.
//
// Fail-safe by construction: only a requirement we can prove is missing blocks.
// A probe that returns nothing reads as `unknown` and surfaces as a warning, so
// a machine that is actually fine but whose probe failed is never hard-blocked
// (issue open question: "don't scare off users whose machines are fine").

export type Platform = "windows" | "macos" | "linux";

/** A hard requirement blocks launch when missing; a soft one only warns. */
export type Severity = "hard" | "soft";

/**
 * `ok` — the requirement is satisfied.
 * `missing` — proven absent; blocks launch only when the requirement is hard.
 * `degraded` — present but below the recommended bar; always a soft warning.
 * `unknown` — the probe returned nothing; surfaced as a warning, never a block,
 *   so a failed probe cannot lock a working machine out.
 */
export type CheckStatus = "ok" | "missing" | "degraded" | "unknown";

/** A detected hardware accelerator, or `none`/`cpu` when there is no useful GPU path. */
export type Acceleration =
  | "vulkan"
  | "directml"
  | "metal"
  | "openblas"
  | "cpu"
  | "none";

/**
 * What the platform adapter probes. Every field except `platform` is optional:
 * a field the adapter could not read is left undefined and evaluates to
 * `unknown`, never to a failing value. `webview2Present` and `runtimeDllsPresent`
 * are Windows-only and ignored elsewhere.
 */
export interface SystemFacts {
  platform: Platform;
  /** CPU/OS architecture, e.g. "x64" or "arm64". */
  arch?: string;
  totalRamMb?: number;
  freeDiskMb?: number;
  /** The WebView2 runtime the Windows UI needs (#39). */
  webview2Present?: boolean;
  /** VC++ CRT and the Vulkan loader the Windows engines need (#36, #44). */
  runtimeDllsPresent?: boolean;
  /** The accelerator the app detected, or "none"/"cpu" when there is no GPU path. */
  acceleration?: Acceleration;
}

export interface RequirementResult {
  id: string;
  label: string;
  severity: Severity;
  status: CheckStatus;
  /** Plain-language statement of the current state. */
  message: string;
  /** How to resolve it, when there is a user action. */
  fix?: string;
}

export interface PreflightReport {
  platform: Platform;
  /**
   * The single gate the caller launches on. False only when a hard requirement
   * is proven `missing`; soft shortfalls and unknown probes never set it false.
   */
  launchable: boolean;
  results: RequirementResult[];
  /** Hard requirements that are proven missing — the reasons launch is blocked. */
  blocking: RequirementResult[];
  /** Soft shortfalls and unknown probes — shown as guidance, not a block. */
  warnings: RequirementResult[];
}

// Documented minimums, exported so SYSTEM_REQUIREMENTS.md and the code share one
// source of truth. Grounded in engine reality: whisper models are ~150 MB to
// ~3 GB each and must load into RAM, so a 4 GB machine is the floor (OOM risk on
// the larger models) and 8 GB is comfortable; the disk floor covers the app plus
// at least one model download. These are the proposed baseline this issue asks
// to define, meant to be tuned as real model sizes settle — change them here and
// the doc and every check move together.
export const MIN_RAM_MB = 4096;
export const RECOMMENDED_RAM_MB = 8192;
export const MIN_FREE_DISK_MB = 4096;

const ok = (
  id: string,
  label: string,
  severity: Severity,
  message: string,
): RequirementResult => ({ id, label, severity, status: "ok", message });

// The same x64 machine is named differently by different probes: Rust/Tauri
// (`std::env::consts::ARCH`) reports "x86_64", the Windows environment reports
// "AMD64", and Node's `process.arch` reports "x64". The hard gate must accept all
// of them, or a valid x64 machine whose adapter used the Rust name would be
// blocked at launch — the exact opaque-failure this check exists to prevent.
const X64_ALIASES = new Set(["x64", "x86_64", "amd64"]);

/** Whether a probed architecture string names a 64-bit x86 machine, in any of its spellings. */
export function isX64Arch(arch: string): boolean {
  return X64_ALIASES.has(arch.trim().toLowerCase());
}

/** x64 is the built target; any other architecture cannot run the bundled binaries. */
function checkArch(facts: SystemFacts): RequirementResult {
  const id = "arch";
  const label = "64-bit (x64) processor";
  if (facts.arch === undefined) {
    return {
      id,
      label,
      severity: "hard",
      status: "unknown",
      message: "Could not determine the processor architecture.",
      fix: "AudioBud ships as a 64-bit (x64) build; a 32-bit or ARM machine cannot run it.",
    };
  }
  if (!isX64Arch(facts.arch)) {
    return {
      id,
      label,
      severity: "hard",
      status: "missing",
      message: `This is a ${facts.arch} machine, but AudioBud ships only as a 64-bit (x64) build.`,
      fix: "Run AudioBud on a 64-bit (x64) machine, or build from source for your architecture.",
    };
  }
  return ok(id, label, "hard", "Running on a supported 64-bit (x64) processor.");
}

/** The WebView2 runtime the Windows UI renders in; without it the window never opens (#39). */
function checkWebView2(facts: SystemFacts): RequirementResult {
  const id = "webview2";
  const label = "WebView2 runtime";
  if (facts.webview2Present === undefined) {
    return {
      id,
      label,
      severity: "hard",
      status: "unknown",
      message: "Could not check whether the WebView2 runtime is installed.",
      fix: "If the window fails to open, install the Microsoft Edge WebView2 runtime.",
    };
  }
  if (!facts.webview2Present) {
    return {
      id,
      label,
      severity: "hard",
      status: "missing",
      message: "The Microsoft Edge WebView2 runtime is not installed, so the app window cannot open.",
      fix: "Install the WebView2 runtime from Microsoft, then relaunch AudioBud.",
    };
  }
  return ok(id, label, "hard", "The WebView2 runtime is installed.");
}

/** VC++ CRT and Vulkan loader DLLs the bundled engines link against (#36, #44). */
function checkRuntimeDlls(facts: SystemFacts): RequirementResult {
  const id = "runtime-dlls";
  const label = "Windows runtime libraries";
  if (facts.runtimeDllsPresent === undefined) {
    return {
      id,
      label,
      severity: "hard",
      status: "unknown",
      message: "Could not check for the required Windows runtime libraries.",
      fix: "If transcription fails to start, install the latest Microsoft Visual C++ redistributable.",
    };
  }
  if (!facts.runtimeDllsPresent) {
    return {
      id,
      label,
      severity: "hard",
      status: "missing",
      message: "Required Windows runtime libraries (Visual C++ runtime, Vulkan loader) are missing, so transcription cannot start.",
      fix: "Install the latest Microsoft Visual C++ redistributable, then relaunch AudioBud.",
    };
  }
  return ok(id, label, "hard", "The required Windows runtime libraries are present.");
}

/** RAM is soft: too little never blocks, but it warns and steers toward a smaller model. */
function checkRam(facts: SystemFacts): RequirementResult {
  const id = "ram";
  const label = "Memory (RAM)";
  const ram = facts.totalRamMb;
  if (ram === undefined) {
    return {
      id,
      label,
      severity: "soft",
      status: "unknown",
      message: "Could not read how much memory this machine has.",
    };
  }
  if (ram >= RECOMMENDED_RAM_MB) {
    return ok(id, label, "soft", `${gb(ram)} of RAM — comfortable for the larger models.`);
  }
  if (ram >= MIN_RAM_MB) {
    return {
      id,
      label,
      severity: "soft",
      status: "degraded",
      message: `${gb(ram)} of RAM is below the recommended ${gb(RECOMMENDED_RAM_MB)}.`,
      fix: "AudioBud will run; choose a smaller model to stay well within memory.",
    };
  }
  return {
    id,
    label,
    severity: "soft",
    status: "degraded",
    message: `${gb(ram)} of RAM is below the ${gb(MIN_RAM_MB)} minimum; larger models may run out of memory mid-transcription.`,
    fix: "Use the smallest model, or run AudioBud on a machine with more memory.",
  };
}

/** Free disk is soft: models are large downloads, so a shortfall warns rather than blocks. */
function checkDisk(facts: SystemFacts): RequirementResult {
  const id = "disk";
  const label = "Free disk space";
  const free = facts.freeDiskMb;
  if (free === undefined) {
    return {
      id,
      label,
      severity: "soft",
      status: "unknown",
      message: "Could not read available disk space.",
    };
  }
  if (free >= MIN_FREE_DISK_MB) {
    return ok(id, label, "soft", `${gb(free)} free — enough for the app and a model.`);
  }
  return {
    id,
    label,
    severity: "soft",
    status: "degraded",
    message: `${gb(free)} free is below the ${gb(MIN_FREE_DISK_MB)} a model download needs.`,
    fix: "Free up disk space before downloading a model.",
  };
}

/** Acceleration is soft: no GPU path means slower transcription, not a broken app. */
function checkAcceleration(facts: SystemFacts): RequirementResult {
  const id = "acceleration";
  const label = "Hardware acceleration";
  const accel = facts.acceleration;
  if (accel === undefined) {
    return {
      id,
      label,
      severity: "soft",
      status: "unknown",
      message: "Could not detect a hardware accelerator.",
    };
  }
  if (accel === "none" || accel === "cpu") {
    return {
      id,
      label,
      severity: "soft",
      status: "degraded",
      message: "No GPU acceleration was found; transcription will run on the CPU and be slower.",
      fix: "A smaller model keeps CPU-only transcription responsive.",
    };
  }
  return ok(id, label, "soft", `Using ${accel} acceleration.`);
}

function gb(mb: number): string {
  return `${(mb / 1024).toFixed(mb % 1024 === 0 ? 0 : 1)} GB`;
}

/**
 * The checks that apply to each platform. Windows is the validated target and
 * carries the full hard gate (arch, WebView2, runtime DLLs); macOS and Linux are
 * inherited from upstream Handy and not yet validated here, so they run only the
 * soft, non-blocking checks — an unvalidated platform should warn, never claim a
 * hard pass it has not earned.
 */
function checksFor(platform: Platform): Array<(f: SystemFacts) => RequirementResult> {
  if (platform === "windows") {
    return [checkArch, checkWebView2, checkRuntimeDlls, checkRam, checkDisk, checkAcceleration];
  }
  return [checkRam, checkDisk, checkAcceleration];
}

/**
 * Evaluate the probed facts into a preflight report. `launchable` is false only
 * when a hard requirement is proven missing; soft shortfalls and unknown probes
 * populate `warnings` and leave launch allowed. The report is deterministic in
 * the order of `checksFor`, so the first-run panel renders a stable list.
 */
export function evaluatePreflight(facts: SystemFacts): PreflightReport {
  const results = checksFor(facts.platform).map((check) => check(facts));
  const blocking = results.filter((r) => r.severity === "hard" && r.status === "missing");
  const warnings = results.filter(
    (r) => r.status === "degraded" || r.status === "unknown",
  );
  return {
    platform: facts.platform,
    launchable: blocking.length === 0,
    results,
    blocking,
    warnings,
  };
}
