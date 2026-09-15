/** Linux is a development host for the frontend, not an application target. */
export const linuxBuildMessage =
  "AudioBud no longer supports Linux application builds or Nix packaging. See docs/platform-support.md. Frontend-only development remains available with bun run dev or bun run build.";

export function nativeTargetError(
  args: readonly string[],
  host: string,
  configuredTarget?: string,
): string | null {
  if (!["dev", "build", "bundle"].includes(args[0] ?? "")) return null;
  const nativeArgs = args.slice(
    0,
    args.indexOf("--") < 0 ? args.length : args.indexOf("--"),
  );
  if (nativeArgs.includes("--help") || nativeArgs.includes("-h")) return null;
  let target = configuredTarget;
  for (let i = 1; i < nativeArgs.length; i++) {
    const arg = nativeArgs[i];
    if (arg === "--target" || arg === "-t") target = nativeArgs[++i];
    else if (arg.startsWith("--target="))
      target = arg.slice("--target=".length);
  }
  if (target) return /(^|-)linux(-|$)/.test(target) ? linuxBuildMessage : null;
  return host === "linux" ? linuxBuildMessage : null;
}
