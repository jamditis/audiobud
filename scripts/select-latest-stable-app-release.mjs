import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

const stableAppTag = /^v\d+\.\d+\.\d+$/;

export function selectLatestStableAppRelease(releasePages) {
  if (!Array.isArray(releasePages)) {
    return null;
  }

  const releases = releasePages.flatMap((page) =>
    Array.isArray(page) ? page : [page],
  );
  let latest = null;

  for (const release of releases) {
    if (
      release === null ||
      typeof release !== "object" ||
      release.draft !== false ||
      release.prerelease !== false ||
      typeof release.tag_name !== "string" ||
      !stableAppTag.test(release.tag_name) ||
      typeof release.published_at !== "string"
    ) {
      continue;
    }

    const publishedAt = Date.parse(release.published_at);
    if (!Number.isFinite(publishedAt)) {
      continue;
    }

    if (latest === null || publishedAt > latest.publishedAt) {
      latest = { publishedAt, tag: release.tag_name };
    }
  }

  return latest?.tag ?? null;
}

function run() {
  try {
    const source = process.argv[2]
      ? readFileSync(process.argv[2], "utf8")
      : readFileSync(0, "utf8");
    const latestTag = selectLatestStableAppRelease(JSON.parse(source));
    if (latestTag === null) {
      console.error("No stable app release was found.");
      process.exitCode = 1;
      return;
    }

    process.stdout.write(`${latestTag}\n`);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    console.error(`Could not select the latest stable app release: ${message}`);
    process.exitCode = 1;
  }
}

if (
  process.argv[1] &&
  pathToFileURL(process.argv[1]).href === import.meta.url
) {
  run();
}
