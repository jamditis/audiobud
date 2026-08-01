import { describe, expect, test } from "bun:test";
import { validateUpdaterKeyTransition } from "./validate-updater-key-transition";

const oldKey = "old-outer-base64-public-key";
const newKey = "new-outer-base64-public-key";
const version = "0.4.2";

describe("updater signing key transitions", () => {
  test("accepts a normal release only when the signing and client keys match", () => {
    expect(
      validateUpdaterKeyTransition({
        appVersion: version,
        signingPublicKey: oldKey,
        clientPublicKey: oldKey,
        bridge: null,
      }),
    ).toBe("normal");

    expect(() =>
      validateUpdaterKeyTransition({
        appVersion: version,
        signingPublicKey: oldKey,
        clientPublicKey: newKey,
        bridge: null,
      }),
    ).toThrow(/does not match.*bridge/i);
  });

  test("accepts an exact, version-pinned bridge transition", () => {
    expect(
      validateUpdaterKeyTransition({
        appVersion: version,
        signingPublicKey: oldKey,
        clientPublicKey: newKey,
        bridge: {
          version,
          signing_public_key: oldKey,
          client_public_key: newKey,
        },
      }),
    ).toBe("bridge");
  });

  test("rejects stale or inaccurate bridge declarations", () => {
    const validBridge = {
      version,
      signing_public_key: oldKey,
      client_public_key: newKey,
    };

    for (const bridge of [
      { ...validBridge, version: "0.4.1" },
      { ...validBridge, signing_public_key: newKey },
      { ...validBridge, client_public_key: oldKey },
    ]) {
      expect(() =>
        validateUpdaterKeyTransition({
          appVersion: version,
          signingPublicKey: oldKey,
          clientPublicKey: newKey,
          bridge,
        }),
      ).toThrow(/bridge/i);
    }

    expect(() =>
      validateUpdaterKeyTransition({
        appVersion: version,
        signingPublicKey: oldKey,
        clientPublicKey: oldKey,
        bridge: validBridge,
      }),
    ).toThrow(/stale/i);
  });
});
