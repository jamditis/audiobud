import { describe, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import {
  existsSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

const workflow = readFileSync(
  ".github/workflows/publish-update-feed.yml",
  "utf8",
);
const releaseWorkflow = readFileSync(".github/workflows/release.yml", "utf8");
const tauriConfig = JSON.parse(
  readFileSync("src-tauri/tauri.conf.json", "utf8"),
);

function stepPosition(name: string): number {
  const position = workflow.indexOf(`- name: ${name}`);
  expect(position, `Missing workflow step: ${name}`).toBeGreaterThan(-1);
  return position;
}

function stepBlock(name: string): string {
  const position = stepPosition(name);
  const next = workflow.indexOf("\n      - name:", position + 1);
  return workflow.slice(position, next === -1 ? undefined : next);
}

function stepScript(name: string): string {
  const block = stepBlock(name);
  const marker = "        run: |\n";
  const start = block.indexOf(marker);
  expect(start, `Missing shell script for step: ${name}`).toBeGreaterThan(-1);
  const scriptLines: string[] = [];
  for (const line of block.slice(start + marker.length).split("\n")) {
    if (line.startsWith("          ")) {
      scriptLines.push(line.slice(10));
    } else if (line.length === 0) {
      scriptLines.push("");
    } else {
      break;
    }
  }
  return scriptLines.join("\n");
}

function runGate(overrides: Record<string, string>) {
  const directory = mkdtempSync(join(tmpdir(), "audiobud-feed-gate-"));
  const scriptPath = join(directory, "gate.sh");
  const outputPath = join(directory, "output.txt");
  writeFileSync(scriptPath, stepScript("Gate stable app release"));
  const result = Bun.spawnSync({
    cmd: ["bash", scriptPath],
    env: {
      ...process.env,
      EVENT_NAME: "workflow_dispatch",
      EVENT_TAG: "",
      EXPECTED_LIVE_SHA256: "",
      GITHUB_OUTPUT: outputPath,
      INPUT_CONFIRM_ROLLBACK: "false",
      INPUT_MANIFEST_SHA256: "",
      INPUT_OPERATION: "publish",
      INPUT_TAG: "v0.6.0",
      ...overrides,
    },
    stderr: "pipe",
    stdout: "pipe",
  });
  const output = existsSync(outputPath) ? readFileSync(outputPath, "utf8") : "";
  rmSync(directory, { force: true, recursive: true });
  return { exitCode: result.exitCode, output };
}

function runRollbackPromotion(options: {
  failTargetReadback: boolean;
  restoreUploadFailures: number;
}) {
  const directory = mkdtempSync(join(tmpdir(), "audiobud-feed-rollback-"));
  const backupDirectory = join(directory, "backup");
  const sourceDirectory = join(directory, "source");
  const assetDirectory = join(directory, "assets");
  const backupPath = join(backupDirectory, "latest.json");
  const rollbackPath = join(
    sourceDirectory,
    `latest-v0.5.0-sha256-${"a".repeat(64)}.json`,
  );
  const livePath = join(directory, "live.json");
  const historyTouchedPath = join(directory, "history-touched.txt");
  const uploadCountPath = join(directory, "upload-count.txt");
  const scriptPath = join(directory, "rollback.sh");
  const currentBytes = '{"version":"0.6.0"}\n';
  const rollbackBytes = '{"version":"0.5.0"}\n';
  Bun.spawnSync({
    cmd: ["mkdir", "-p", backupDirectory, sourceDirectory, assetDirectory],
  });
  writeFileSync(backupPath, currentBytes);
  writeFileSync(rollbackPath, rollbackBytes);
  writeFileSync(livePath, currentBytes);
  const backupSha256 = createHash("sha256").update(currentBytes).digest("hex");
  writeFileSync(
    scriptPath,
    `BACKUP_MANIFEST=${JSON.stringify(backupPath)}
BACKUP_SHA256=${JSON.stringify(backupSha256)}
FEED_TAG=update-feed
GITHUB_REPOSITORY=jamditis/audiobud
ROLLBACK_MANIFEST=${JSON.stringify(rollbackPath)}
RUNNER_TEMP=${JSON.stringify(directory)}
LIVE_MANIFEST=${JSON.stringify(livePath)}
ASSET_DIRECTORY=${JSON.stringify(assetDirectory)}
HISTORY_TOUCHED_PATH=${JSON.stringify(historyTouchedPath)}
UPLOAD_COUNT_PATH=${JSON.stringify(uploadCountPath)}
FAIL_TARGET_READBACK=${options.failTargetReadback ? "true" : "false"}
RESTORE_FAILURES_REMAINING=${options.restoreUploadFailures}
UPLOAD_COUNT=0
DOWNLOAD_COUNT=0
trap 'printf "%s" "$UPLOAD_COUNT" > "$UPLOAD_COUNT_PATH"' EXIT
sleep() { :; }
gh() {
  if [[ "$1 $2" == "release upload" ]]; then
    UPLOAD_COUNT=$((UPLOAD_COUNT + 1))
    local source_path="$4"
    local asset_name
    asset_name=$(basename "$source_path")
    if [[ "$asset_name" != "latest.json" ]]; then
      cp -- "$source_path" "$ASSET_DIRECTORY/$asset_name"
      touch "$HISTORY_TOUCHED_PATH"
      return 0
    fi
    if [[ "$UPLOAD_COUNT" -gt 1 && "$RESTORE_FAILURES_REMAINING" -gt 0 ]]; then
      RESTORE_FAILURES_REMAINING=$((RESTORE_FAILURES_REMAINING - 1))
      return 1
    fi
    cp -- "$source_path" "$LIVE_MANIFEST"
    return 0
  fi
  if [[ "$1 $2" == "release download" ]]; then
    DOWNLOAD_COUNT=$((DOWNLOAD_COUNT + 1))
    shift 2
    local output_directory=""
    while [[ $# -gt 0 ]]; do
      if [[ "$1" == "--dir" ]]; then
        output_directory="$2"
        break
      fi
      shift
    done
    if [[ "$FAIL_TARGET_READBACK" == "true" && "$DOWNLOAD_COUNT" -eq 2 ]]; then
      return 1
    fi
    cp -- "$LIVE_MANIFEST" "$output_directory/latest.json"
    return 0
  fi
  return 1
}
${stepScript("Restore and verify saved update feed")}
`,
  );
  const result = Bun.spawnSync({
    cmd: ["bash", scriptPath],
    stderr: "pipe",
    stdout: "pipe",
  });
  const state = {
    exitCode: result.exitCode,
    historyTouched: existsSync(historyTouchedPath),
    liveBytes: readFileSync(livePath, "utf8"),
    uploadCount: Number(readFileSync(uploadCountPath, "utf8")),
  };
  rmSync(directory, { force: true, recursive: true });
  return state;
}

describe("published update feed workflow", () => {
  test("runs only after publication or an explicit stable-tag retry", () => {
    expect(workflow).toContain("release:\n    types: [published]");
    expect(workflow).toContain("workflow_dispatch:");
    expect(workflow).toContain("tag:");
    expect(workflow).toContain("^v[0-9]+\\.[0-9]+\\.[0-9]+$");
    expect(workflow).toContain('echo "publish=false"');
    expect(workflow).toContain("if: needs.gate.outputs.publish == 'true'");
    expect(workflow).toContain(
      ".draft or .prerelease or (.published_at == null)",
    );
    expect(
      workflow.match(/gh api "repos\/\$GITHUB_REPOSITORY\/releases\/latest"/g),
    ).toHaveLength(2);
    expect(workflow).toContain(
      'LATEST_TAG=$(jq -r ".tag_name" "$LATEST_JSON")',
    );
    expect(workflow).toContain('if [[ "$LATEST_TAG" != "$TAG" ]]');
    expect(workflow).toContain("operation:");
    expect(workflow).toContain("options:");
    expect(workflow).toContain("- publish");
    expect(workflow).toContain("- rollback");
    expect(workflow).toContain("manifest_sha256:");
    expect(workflow).toContain("expected_live_sha256:");
    expect(workflow).toContain("confirm_rollback:");
  });

  test("rejects unsafe dispatch input combinations before any feed job", () => {
    expect(runGate({})).toEqual({
      exitCode: 0,
      output: "tag=v0.6.0\npublish=true\nrollback=false\n",
    });

    const digest = "a".repeat(64);
    expect(
      runGate({
        EXPECTED_LIVE_SHA256: "b".repeat(64),
        INPUT_CONFIRM_ROLLBACK: "true",
        INPUT_MANIFEST_SHA256: digest,
        INPUT_OPERATION: "rollback",
        INPUT_TAG: "v0.5.0",
      }),
    ).toEqual({
      exitCode: 0,
      output: "publish=false\nrollback=true\ntag=v0.5.0\n",
    });
    expect(
      runGate({ INPUT_OPERATION: "rollback", INPUT_TAG: "v0.5.0" }).exitCode,
    ).not.toBe(0);
    expect(runGate({ INPUT_MANIFEST_SHA256: digest }).exitCode).not.toBe(0);
    expect(runGate({ INPUT_TAG: "v0.6" }).exitCode).not.toBe(0);
  });

  test("verifies exact updater assets before it builds the manifest", () => {
    expect(stepPosition("Resolve published release")).toBeLessThan(
      stepPosition("Download signed updater assets"),
    );
    expect(stepPosition("Download signed updater assets")).toBeLessThan(
      stepPosition("Verify updater provenance"),
    );
    expect(stepPosition("Verify updater provenance")).toBeLessThan(
      stepPosition("Verify updater signature"),
    );
    expect(stepPosition("Verify updater signature")).toBeLessThan(
      stepPosition("Require successful private updater evidence"),
    );
    expect(
      stepPosition("Require successful private updater evidence"),
    ).toBeLessThan(stepPosition("Generate latest.json"));
    expect(stepPosition("Generate latest.json")).toBeLessThan(
      stepPosition("Upload prepared update manifest"),
    );

    expect(workflow).toContain('gh release download "$TAG"');
    expect(workflow).toContain('--pattern "*.nsis.zip"');
    expect(workflow).toContain('--pattern "*.nsis.zip.sig"');
    expect(workflow).toContain('--pattern "updater-signing-public-key.pub"');
    expect(workflow).toContain("gh attestation verify");
    expect(workflow).toContain('--source-ref "refs/tags/$TAG"');
    expect(workflow).toContain('--source-digest "$RELEASE_COMMIT"');
    expect(workflow).toContain("--deny-self-hosted-runners");
    expect(workflow).toContain("scripts/verify-updater-signature/Cargo.toml");
    expect(workflow).toContain("cargo run --locked --release");
    expect(workflow).toContain("scripts/generate-update-manifest.ts");
    expect(workflow).toContain('MANIFEST="$RUNNER_TEMP/latest.json"');
    const manifest = stepBlock("Generate latest.json");
    expect(manifest).toContain('VERSION="${TAG#v}"');
    expect(manifest).toContain(
      'EXPECTED_ARCHIVE_URL="https://github.com/$GITHUB_REPOSITORY/releases/download/$TAG/AudioBud_${VERSION}_x64-setup.nsis.zip"',
    );
    expect(manifest).toContain(".version == $version");
    expect(manifest).toContain(
      '.platforms["windows-x86_64"].url == $archive_url',
    );
    expect(manifest).toContain(
      '.platforms["windows-x86_64"].signature == $signature',
    );
    expect(releaseWorkflow).not.toContain("- name: Generate latest.json");
    expect(releaseWorkflow).not.toContain("- name: Publish latest.json");
  });

  test("requires evidence from the exact successful release run", () => {
    expect(workflow).toContain("actions: read");
    expect(workflow).toContain("attestations: read");
    expect(workflow).toContain(
      '--pattern "updater-prepublication-evidence.json"',
    );
    expect(workflow).toContain("steps.updater.outputs.evidence");
    expect(workflow).toContain("actions/runs/$RUN_ID/attempts/$RUN_ATTEMPT");
    expect(workflow).toContain("run_attempt");
    expect(workflow).toContain('.conclusion == "success"');
    expect(workflow).toContain(".head_sha == $commit");
    expect(workflow).toContain(".head_branch == $tag");
    expect(workflow).toContain("updater-prepublication-evidence.json");
    expect(workflow).toContain(".target_commit == $commit");
    expect(workflow).toContain(".target_tag == $tag");
    expect(workflow).toContain('.prior_tag == "v0.5.0"');
    expect(workflow).toContain(".workflow_run_attempt == $run_attempt");
    expect(workflow).toContain(".updater_archive_sha256 == $archive_sha256");
    expect(workflow).toContain(".settings_value_before == true");
    expect(workflow).toContain(".model_sha256_before == .model_sha256_after");
    expect(workflow).toContain(".uninstall_passed == true");
    expect(workflow).toContain(
      '(.installed_version == $version or .installed_version == ($version + ".0"))',
    );
    expect(workflow).not.toContain("gh run list");
    expect(workflow).not.toContain("gh run download");
    expect(workflow).not.toContain("eligible=false");
  });

  test("keeps the prepared manifest inside the workflow run", () => {
    expect(workflow).not.toContain("latest-candidate.json");
    expect(workflow).not.toContain("Upload candidate manifest");
    expect(workflow).not.toContain("verify_update:");
    expect(workflow).toContain("Upload prepared update manifest");
    expect(workflow).toContain(
      "artifact-name: ${{ steps.manifest.outputs.artifact_name }}",
    );
    expect(workflow).toContain(
      "audiobud-update-manifest-$GITHUB_RUN_ID-$GITHUB_RUN_ATTEMPT",
    );
    expect(workflow).toContain(
      "name: ${{ needs.prepare.outputs.artifact-name }}",
    );
    expect(workflow).not.toContain(
      "name: audiobud-update-manifest-${{ github.run_id }}",
    );
    expect(workflow).toContain(
      "actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02",
    );
    expect(workflow).toContain(
      "actions/download-artifact@d3f86a106a0bac45b974a628896c90dbdf5c8093",
    );
    expect(stepPosition("Download prepared update manifest")).toBeLessThan(
      stepPosition("Backup live update feed"),
    );
    expect(stepPosition("Backup live update feed")).toBeLessThan(
      stepPosition("Promote and verify update feed"),
    );
  });

  test("restores exact prior feed bytes if promotion fails", () => {
    const promotion = stepBlock("Promote and verify update feed");
    expect(promotion).toContain("restore_previous_feed");
    expect(promotion).toContain("trap restore_previous_feed ERR");
    expect(promotion).toContain('gh release upload "$FEED_TAG"');
    expect(promotion.match(/cmp --silent/g)).toHaveLength(2);
    expect(promotion).toContain("BACKUP_SHA256");
    expect(promotion).toContain("trap - ERR");
    const restoreStart = promotion.indexOf("restore_previous_feed() {");
    const restoreEnd = promotion.indexOf(
      "trap restore_previous_feed ERR",
      restoreStart,
    );
    const restore = promotion.slice(restoreStart, restoreEnd);
    expect(restoreStart).toBeGreaterThan(-1);
    expect(restoreEnd).toBeGreaterThan(restoreStart);
    expect(restore).toContain("set +e");
    expect(restore).toContain("for restore_attempt in 1 2 3");
    expect(restore).toContain('if gh release upload "$FEED_TAG"');
    expect(restore).not.toMatch(/^\s*return\b/m);
    expect(restore.match(/^\s*exit (?:1|"\$failure_status")$/gm)).toHaveLength(
      5,
    );
    expect(restore).toContain(
      'if ! cmp --silent "$BACKUP_MANIFEST" "$RESTORED_MANIFEST"; then',
    );
    expect(restore).toContain(
      "Restored update feed bytes do not match the backup",
    );
    expect(restore.indexOf("set +e")).toBeLessThan(
      restore.indexOf('gh release upload "$FEED_TAG"'),
    );
    expect(
      restore.indexOf(
        'if ! cmp --silent "$BACKUP_MANIFEST" "$RESTORED_MANIFEST"; then',
      ),
    ).toBeLessThan(restore.indexOf("RESTORED_SHA256="));
    expect(workflow).toContain("FEED_TAG: update-feed");
  });

  test("a trapped promotion failure restores bytes and cannot resume", () => {
    const promotion = stepScript("Promote and verify update feed");
    const functionStart = promotion.indexOf("restore_previous_feed() {");
    const functionEnd = promotion.indexOf(
      "trap restore_previous_feed ERR",
      functionStart,
    );
    expect(functionStart).toBeGreaterThan(-1);
    expect(functionEnd).toBeGreaterThan(functionStart);
    const restoreFunction = promotion.slice(functionStart, functionEnd);

    const directory = mkdtempSync(join(tmpdir(), "audiobud-feed-trap-"));
    const backupPath = join(directory, "latest.json");
    const livePath = join(directory, "live.json");
    const afterPath = join(directory, "after.txt");
    const scriptPath = join(directory, "trap.sh");
    const backupBytes = '{"version":"0.5.0"}\n';
    writeFileSync(backupPath, backupBytes);
    writeFileSync(livePath, '{"version":"broken"}\n');
    const backupSha256 = createHash("sha256").update(backupBytes).digest("hex");
    writeFileSync(
      scriptPath,
      `set -eEuo pipefail
BACKUP_MANIFEST=${JSON.stringify(backupPath)}
BACKUP_SHA256=${JSON.stringify(backupSha256)}
FEED_TAG=update-feed
GITHUB_REPOSITORY=jamditis/audiobud
RUNNER_TEMP=${JSON.stringify(directory)}
LIVE_MANIFEST=${JSON.stringify(livePath)}
AFTER_PATH=${JSON.stringify(afterPath)}
gh() {
  if [[ "$1 $2" == "release upload" ]]; then
    cp -- "$4" "$LIVE_MANIFEST"
    return 0
  fi
  if [[ "$1 $2" == "release download" ]]; then
    shift 2
    local output_directory=""
    while [[ $# -gt 0 ]]; do
      if [[ "$1" == "--dir" ]]; then
        output_directory="$2"
        break
      fi
      shift
    done
    cp -- "$LIVE_MANIFEST" "$output_directory/latest.json"
    return 0
  fi
  return 1
}
${restoreFunction}
trap restore_previous_feed ERR
false
printf 'resumed' > "$AFTER_PATH"
`,
    );

    const result = Bun.spawnSync({
      cmd: ["bash", scriptPath],
      stderr: "pipe",
      stdout: "pipe",
    });
    expect(result.exitCode).not.toBe(0);
    expect(readFileSync(livePath, "utf8")).toBe(backupBytes);
    expect(existsSync(afterPath)).toBe(false);
    rmSync(directory, { force: true, recursive: true });
  });

  test("keeps an exact durable backup before each feed mutation", () => {
    const backup = stepBlock("Backup live update feed");
    expect(backup).toContain('BACKUP_VERSION=$(jq -r ".version // empty"');
    expect(backup).toContain(
      'BACKUP_ASSET_NAME="latest-v${BACKUP_VERSION}-sha256-${BACKUP_SHA256}.json"',
    );
    expect(backup).toContain('gh release upload "$FEED_TAG" "$DURABLE_BACKUP"');
    expect(backup).not.toContain("--clobber");
    expect(backup).toContain('gh release download "$FEED_TAG"');
    expect(backup).toContain('cmp --silent "$BACKUP_MANIFEST" "$SAVED_BACKUP"');
    expect(backup).toContain("$GITHUB_STEP_SUMMARY");
    expect(backup).toContain('sub("\\\\.[0-9]+Z$"; "Z")');
  });

  test("supports an exact hash-checked rollback from a durable backup", () => {
    expect(workflow).toContain("rollback:");
    expect(workflow).toContain("needs.gate.outputs.rollback == 'true'");
    const rollback = workflow.slice(workflow.indexOf("\n  rollback:\n"));
    expect(rollback).toContain(
      "MANIFEST_SHA256: ${{ inputs.manifest_sha256 }}",
    );
    expect(rollback).toContain(
      "EXPECTED_LIVE_SHA256: ${{ inputs.expected_live_sha256 }}",
    );
    expect(rollback).toContain(
      'ROLLBACK_ASSET="latest-${TAG}-sha256-${MANIFEST_SHA256}.json"',
    );
    expect(rollback).toContain(
      'ACTUAL_SHA256=$(sha256sum "$ROLLBACK_MANIFEST"',
    );
    expect(rollback).toContain(
      'if [[ "$ACTUAL_SHA256" != "$MANIFEST_SHA256" ]]',
    );
    expect(rollback).toContain(
      'EXPECTED_URL="https://github.com/$GITHUB_REPOSITORY/releases/download/$TAG/$ARCHIVE_NAME"',
    );
    expect(rollback).toContain("gh attestation verify");
    expect(rollback).toContain('sub("\\\\.[0-9]+Z$"; "Z")');
    expect(rollback).toContain("scripts/verify-updater-signature/Cargo.toml");
    expect(rollback).toContain("Save current feed before rollback");
    expect(rollback).toContain(
      'PUBLISH_MANIFEST="$RUNNER_TEMP/update-feed-rollback-publish/latest.json"',
    );
    expect(rollback).toContain(
      'cp -- "$ROLLBACK_MANIFEST" "$PUBLISH_MANIFEST"',
    );
    expect(rollback).toContain(
      'gh release upload "$FEED_TAG" "$PUBLISH_MANIFEST"',
    );
    expect(rollback).toContain('--clobber --repo "$GITHUB_REPOSITORY"');
    expect(rollback).toContain(
      'cmp --silent "$PUBLISH_MANIFEST" "$READBACK_MANIFEST"',
    );
    const restoreStart = rollback.indexOf("restore_outgoing_feed() {");
    const restoreEnd = rollback.indexOf(
      "trap restore_outgoing_feed ERR",
      restoreStart,
    );
    const restore = rollback.slice(restoreStart, restoreEnd);
    expect(restore).toContain("for restore_attempt in 1 2 3");
    expect(restore).toContain("Restore upload attempt $restore_attempt failed");
  });

  test("rollback changes only latest.json to the exact saved bytes", () => {
    expect(
      runRollbackPromotion({
        failTargetReadback: false,
        restoreUploadFailures: 0,
      }),
    ).toEqual({
      exitCode: 0,
      historyTouched: false,
      liveBytes: '{"version":"0.5.0"}\n',
      uploadCount: 1,
    });
  });

  test("rollback retries a transient outgoing-feed restore and exits red", () => {
    const result = runRollbackPromotion({
      failTargetReadback: true,
      restoreUploadFailures: 1,
    });
    expect(result.exitCode).not.toBe(0);
    expect(result.historyTouched).toBe(false);
    expect(result.liveBytes).toBe('{"version":"0.6.0"}\n');
    expect(result.uploadCount).toBe(3);
  });

  test("pins every third-party action", () => {
    const actions = [
      ...workflow.matchAll(/^\s*uses:\s+([^@\s]+)@([^\s#]+)(?:\s+#.*)?$/gm),
    ];
    expect(actions.length).toBeGreaterThan(0);
    for (const [, name, reference] of actions) {
      expect(reference, `${name} must use a full commit SHA`).toMatch(
        /^[0-9a-f]{40}$/,
      );
    }
  });

  test("publishes to the endpoint embedded in AudioBud", () => {
    expect(tauriConfig.plugins.updater.endpoints).toEqual([
      "https://github.com/jamditis/audiobud/releases/download/update-feed/latest.json",
    ]);
    expect(workflow).toContain(
      'gh release upload "$FEED_TAG" "$PREPARED_MANIFEST"',
    );
    expect(workflow).toContain(
      'cmp --silent "$PREPARED_MANIFEST" "$READBACK_MANIFEST"',
    );
  });
});
