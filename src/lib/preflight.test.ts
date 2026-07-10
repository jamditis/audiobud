import { describe, it, expect } from "bun:test";
import {
  evaluatePreflight,
  isX64Arch,
  MIN_RAM_MB,
  RECOMMENDED_RAM_MB,
  MIN_FREE_DISK_MB,
  type SystemFacts,
} from "./preflight";

// These prove the acceptance criteria of #51, not just line coverage: a proven
// missing HARD requirement blocks with an actionable message, a SOFT shortfall
// warns with guidance and never blocks, and a probe that could not read a value
// surfaces as a warning rather than locking out a machine that is actually fine.
// Documented minimums: SYSTEM_REQUIREMENTS.md.

// A Windows machine that meets everything, as the baseline to vary from.
const goodWindows = (over: Partial<SystemFacts> = {}): SystemFacts => ({
  platform: "windows",
  arch: "x64",
  totalRamMb: RECOMMENDED_RAM_MB,
  freeDiskMb: MIN_FREE_DISK_MB * 2,
  webview2Present: true,
  runtimeDllsPresent: true,
  acceleration: "directml",
  ...over,
});

describe("hard requirements block launch with an actionable message", () => {
  it("blocks when the WebView2 runtime is missing", () => {
    const report = evaluatePreflight(goodWindows({ webview2Present: false }));
    expect(report.launchable).toBe(false);
    const webview2 = report.blocking.find((r) => r.id === "webview2");
    expect(webview2?.severity).toBe("hard");
    expect(webview2?.status).toBe("missing");
    expect(webview2?.fix).toMatch(/WebView2/i);
  });

  it("blocks when the Windows runtime DLLs are missing", () => {
    const report = evaluatePreflight(goodWindows({ runtimeDllsPresent: false }));
    expect(report.launchable).toBe(false);
    expect(report.blocking.map((r) => r.id)).toContain("runtime-dlls");
  });

  it("blocks a non-x64 machine", () => {
    const report = evaluatePreflight(goodWindows({ arch: "arm64" }));
    expect(report.launchable).toBe(false);
    const arch = report.blocking.find((r) => r.id === "arch");
    expect(arch?.message).toMatch(/arm64/);
  });

  it("reports every missing hard requirement at once, not just the first", () => {
    const report = evaluatePreflight(
      goodWindows({ webview2Present: false, runtimeDllsPresent: false, arch: "x86" }),
    );
    expect(report.launchable).toBe(false);
    expect(report.blocking.map((r) => r.id).sort()).toEqual([
      "arch",
      "runtime-dlls",
      "webview2",
    ]);
  });
});

describe("the arch gate accepts x64 under every probe's spelling", () => {
  // The same x64 machine is named "x64" (Node), "x86_64" (Rust/Tauri), or
  // "AMD64" (Windows env) depending on which probe the adapter used. All three
  // must pass, or a valid target machine is blocked at launch.
  it.each(["x64", "x86_64", "amd64", "AMD64", "  x86_64  "])(
    "treats %p as a supported 64-bit processor",
    (arch) => {
      const report = evaluatePreflight(goodWindows({ arch }));
      expect(report.launchable).toBe(true);
      expect(report.results.find((r) => r.id === "arch")?.status).toBe("ok");
    },
  );

  it("still blocks genuinely unsupported architectures", () => {
    for (const arch of ["arm64", "aarch64", "x86", "i686"]) {
      const report = evaluatePreflight(goodWindows({ arch }));
      expect(report.launchable).toBe(false);
      expect(report.blocking.map((r) => r.id)).toContain("arch");
    }
  });

  it("isX64Arch matches the aliases and rejects everything else", () => {
    expect(["x64", "x86_64", "amd64", "AMD64", "x86_64 "].every(isX64Arch)).toBe(true);
    expect(["arm64", "aarch64", "x86", "i686", ""].some(isX64Arch)).toBe(false);
  });
});

describe("soft shortfalls warn but never block", () => {
  it("warns on low RAM and steers toward a smaller model, still launchable", () => {
    const low = MIN_RAM_MB + 512; // above the floor, below recommended
    const report = evaluatePreflight(goodWindows({ totalRamMb: low }));
    expect(report.launchable).toBe(true);
    const ram = report.warnings.find((r) => r.id === "ram");
    expect(ram?.severity).toBe("soft");
    expect(ram?.status).toBe("degraded");
    expect(ram?.fix).toMatch(/smaller model/i);
  });

  it("warns harder below the RAM minimum but still does not block", () => {
    const report = evaluatePreflight(goodWindows({ totalRamMb: MIN_RAM_MB - 1024 }));
    expect(report.launchable).toBe(true);
    const ram = report.results.find((r) => r.id === "ram");
    expect(ram?.status).toBe("degraded");
    expect(ram?.message).toMatch(/out of memory/i);
  });

  it("warns on a disk shortfall without blocking", () => {
    const report = evaluatePreflight(goodWindows({ freeDiskMb: MIN_FREE_DISK_MB - 512 }));
    expect(report.launchable).toBe(true);
    expect(report.warnings.map((r) => r.id)).toContain("disk");
  });

  it("warns when there is no GPU acceleration", () => {
    const report = evaluatePreflight(goodWindows({ acceleration: "none" }));
    expect(report.launchable).toBe(true);
    const accel = report.warnings.find((r) => r.id === "acceleration");
    expect(accel?.status).toBe("degraded");
    expect(accel?.message).toMatch(/CPU/i);
  });
});

describe("a fully-capable machine passes clean", () => {
  it("is launchable with no blocking and no warnings", () => {
    const report = evaluatePreflight(goodWindows());
    expect(report.launchable).toBe(true);
    expect(report.blocking).toHaveLength(0);
    expect(report.warnings).toHaveLength(0);
    expect(report.results.every((r) => r.status === "ok")).toBe(true);
  });
});

describe("a failed probe is fail-safe: warn, never block", () => {
  it("does not block when a hard probe returns unknown", () => {
    // A machine that is actually fine but whose WebView2 probe failed must not be
    // locked out — unknown surfaces as a warning, launch stays allowed.
    const report = evaluatePreflight(goodWindows({ webview2Present: undefined }));
    expect(report.launchable).toBe(true);
    const webview2 = report.results.find((r) => r.id === "webview2");
    expect(webview2?.status).toBe("unknown");
    expect(report.warnings.map((r) => r.id)).toContain("webview2");
  });

  it("treats unreadable RAM and disk as warnings, not failures", () => {
    const report = evaluatePreflight(
      goodWindows({ totalRamMb: undefined, freeDiskMb: undefined }),
    );
    expect(report.launchable).toBe(true);
    expect(report.warnings.map((r) => r.id)).toEqual(
      expect.arrayContaining(["ram", "disk"]),
    );
  });
});

describe("unvalidated platforms run only the soft checks", () => {
  it("does not apply the Windows hard gate on macOS", () => {
    const report = evaluatePreflight({
      platform: "macos",
      totalRamMb: RECOMMENDED_RAM_MB,
      freeDiskMb: MIN_FREE_DISK_MB * 2,
      acceleration: "metal",
    });
    expect(report.launchable).toBe(true);
    // No webview2/runtime-dlls/arch hard checks on a non-Windows platform.
    expect(report.results.map((r) => r.id).sort()).toEqual(["acceleration", "disk", "ram"]);
    expect(report.results.some((r) => r.severity === "hard")).toBe(false);
  });
});
