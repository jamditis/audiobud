import { existsSync, readFileSync } from "node:fs";

export interface UpdaterKeyBridge {
  version: string;
  signing_public_key: string;
  client_public_key: string;
}

interface ValidateUpdaterKeyTransitionOptions {
  appVersion: string;
  signingPublicKey: string;
  clientPublicKey: string;
  bridge: UpdaterKeyBridge | null;
}

export function validateUpdaterKeyTransition({
  appVersion,
  signingPublicKey,
  clientPublicKey,
  bridge,
}: ValidateUpdaterKeyTransitionOptions): "normal" | "bridge" {
  const signingKey = signingPublicKey.trim();
  const clientKey = clientPublicKey.trim();

  if (!signingKey) {
    throw new Error("TAURI_SIGNING_PUBLIC_KEY is empty");
  }
  if (!clientKey) {
    throw new Error("The updater key pinned in tauri.conf.json is empty");
  }

  if (signingKey === clientKey) {
    if (bridge) {
      throw new Error(
        "updater-key-bridge.json is stale because the signing and client keys already match",
      );
    }
    return "normal";
  }

  if (!bridge) {
    throw new Error(
      "TAURI_SIGNING_PUBLIC_KEY does not match the updater key pinned in tauri.conf.json, and updater-key-bridge.json is absent",
    );
  }
  if (bridge.version !== appVersion) {
    throw new Error(
      `Updater key bridge version ${bridge.version} does not match app version ${appVersion}`,
    );
  }
  if (bridge.signing_public_key.trim() !== signingKey) {
    throw new Error(
      "Updater key bridge signing_public_key does not match TAURI_SIGNING_PUBLIC_KEY",
    );
  }
  if (bridge.client_public_key.trim() !== clientKey) {
    throw new Error(
      "Updater key bridge client_public_key does not match tauri.conf.json",
    );
  }

  return "bridge";
}

if (import.meta.main) {
  const configPath = "src-tauri/tauri.conf.json";
  const bridgePath = "updater-key-bridge.json";
  const config = JSON.parse(readFileSync(configPath, "utf8")) as {
    version?: unknown;
    plugins?: { updater?: { pubkey?: unknown } };
  };
  const bridge = existsSync(bridgePath)
    ? (JSON.parse(readFileSync(bridgePath, "utf8")) as UpdaterKeyBridge)
    : null;

  const mode = validateUpdaterKeyTransition({
    appVersion: String(config.version ?? ""),
    signingPublicKey: process.env.TAURI_SIGNING_PUBLIC_KEY ?? "",
    clientPublicKey: String(config.plugins?.updater?.pubkey ?? ""),
    bridge,
  });
  console.log(`Validated updater key transition mode: ${mode}`);
}
