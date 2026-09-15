import { nativeTargetError } from "./native-platform";

const args = process.argv.slice(2);
const error = nativeTargetError(
  args,
  process.platform,
  process.env.CARGO_BUILD_TARGET,
);
if (error) {
  console.error(error);
  process.exit(1);
}

try {
  // Resolve the installed CLI, never download a different package version.
  const result = Bun.spawnSync(["bunx", "--no-install", "tauri", ...args], {
    stdin: "inherit",
    stdout: "inherit",
    stderr: "inherit",
  });
  process.exit(result.exitCode);
} catch (cause) {
  console.error(
    "Could not start the installed Tauri CLI. Run bun install --frozen-lockfile.",
    cause,
  );
  process.exit(1);
}
