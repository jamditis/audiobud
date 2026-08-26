import { listen } from "@tauri-apps/api/event";
import React, { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { CancelIcon } from "../components/icons";
import { DEFAULT_CRITTER_ID, getCritter } from "../components/icons/critters";
import "./RecordingOverlay.css";
import { commands, events } from "@/bindings";
import { usePrefersReducedMotion } from "@/hooks/useMicLevel";
import { useOutputTargetLock } from "@/hooks/useOutputTargetLock";
import i18n, { syncLanguageFromSettings } from "@/i18n";
import {
  formatDeliveredWindowName,
  truncateName,
} from "@/lib/output-target-indicator";
import { MIC_LEVEL_EVENT, bandsToAmplitude } from "@/lib/mic-level";
import { getLanguageDirection } from "@/lib/utils/rtl";

// The overlay is a fixed 172px pill, so the locked-target name gets a tighter
// ceiling than the default (settings has the room for the full name).
const OVERLAY_TARGET_NAME_MAX_LENGTH = 12;

// How long the delivered-transcript confirmation (#165) stays on the overlay
// before it is allowed to hide. Long enough to read a short window name at a
// glance, short enough not to linger once the user has moved on.
const DELIVERY_CONFIRMATION_MS = 1800;

type OverlayState = "recording" | "transcribing" | "processing";

// Payload of the Rust `show-overlay` event (see src-tauri/src/overlay.rs). `raw` reflects
// whether the current dictation will be emitted as raw transcript.
type OverlayShowPayload = { state: OverlayState; raw: boolean };

const RecordingOverlay: React.FC = () => {
  const { t } = useTranslation();
  const [isVisible, setIsVisible] = useState(false);
  const [state, setState] = useState<OverlayState>("recording");
  const [isRaw, setIsRaw] = useState(false);
  const [levels, setLevels] = useState<number[]>(Array(16).fill(0));
  const smoothedLevelsRef = useRef<number[]>(Array(16).fill(0));
  // Delivered-transcript confirmation (#165): the resolved window name, or ""
  // when a delivery happened but neither label lookup resolved one, or `null`
  // when there is nothing to show. Read through refs (not this state) inside
  // the event listeners below, which are set up once and would otherwise see
  // a stale closure.
  const [deliveryConfirmation, setDeliveryConfirmation] = useState<
    string | null
  >(null);
  const confirmationActiveRef = useRef(false);
  const pendingHideRef = useRef(false);
  const confirmationTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(
    null,
  );
  const reduceMotion = usePrefersReducedMotion();
  const direction = getLanguageDirection(i18n.language);
  const overlayIndicatorOptions = useRef({
    maxNameLength: OVERLAY_TARGET_NAME_MAX_LENGTH,
  }).current;
  const { indicator: targetLockIndicator, unlock: unlockTarget } =
    useOutputTargetLock(overlayIndicatorOptions);

  useEffect(() => {
    let cleanup: (() => void) | undefined;
    let cancelled = false;

    const setupEventListeners = async () => {
      // Listen for show-overlay event from Rust
      const unlistenShow = await listen("show-overlay", async (event) => {
        // Sync language from settings each time overlay is shown
        await syncLanguageFromSettings();
        const payload = event.payload as OverlayShowPayload;
        setState(payload.state);
        setIsRaw(payload.raw);
        setIsVisible(true);
      });

      // Listen for hide-overlay event from Rust
      const unlistenHide = await listen("hide-overlay", () => {
        if (confirmationActiveRef.current) {
          // A delivery confirmation (#165) is still on screen -- the paste
          // that triggered it finishes, and the overlay's own hide, in the
          // same beat. Let the confirmation's own timeout decide when the
          // overlay actually goes away instead of it vanishing mid-read.
          pendingHideRef.current = true;
          setIsRaw(false);
          return;
        }
        setIsVisible(false);
        setIsRaw(false);
      });

      // Listen for a successful delivery to a pinned window (issue #165). The
      // overlay is the only surface guaranteed visible during the normal
      // tray/global-shortcut flow -- the settings window is usually closed --
      // so it is where this confirmation has to land, not only the toast in
      // App.tsx's hidden webview.
      const unlistenDelivered = await events.transcriptDeliveredEvent.listen(
        (event) => {
          const name = truncateName(
            formatDeliveredWindowName(
              event.payload.app ?? undefined,
              event.payload.title ?? undefined,
            ),
            OVERLAY_TARGET_NAME_MAX_LENGTH,
          );
          confirmationActiveRef.current = true;
          pendingHideRef.current = false;
          setDeliveryConfirmation(name);
          setIsVisible(true);
          if (confirmationTimeoutRef.current) {
            clearTimeout(confirmationTimeoutRef.current);
          }
          confirmationTimeoutRef.current = setTimeout(() => {
            confirmationActiveRef.current = false;
            confirmationTimeoutRef.current = null;
            setDeliveryConfirmation(null);
            if (pendingHideRef.current) {
              pendingHideRef.current = false;
              setIsVisible(false);
            }
          }, DELIVERY_CONFIRMATION_MS);
        },
      );

      // Listen for mic-level updates
      const unlistenLevel = await listen<number[]>(MIC_LEVEL_EVENT, (event) => {
        const newLevels = event.payload as number[];

        // Apply smoothing to reduce jitter
        const smoothed = smoothedLevelsRef.current.map((prev, i) => {
          const target = newLevels[i] || 0;
          return prev * 0.7 + target * 0.3; // Smooth transition
        });

        smoothedLevelsRef.current = smoothed;
        setLevels(smoothed.slice(0, 9));
      });

      const unlistenAll = () => {
        unlistenShow();
        unlistenHide();
        unlistenLevel();
        unlistenDelivered();
        if (confirmationTimeoutRef.current) {
          clearTimeout(confirmationTimeoutRef.current);
          confirmationTimeoutRef.current = null;
        }
      };

      // Unmounted while the subscriptions were still being awaited: drop them
      // now. Returning a cleanup from this async function is not enough on its
      // own -- useEffect receives the promise, not the function, so the
      // subscriptions have to be handed back through `cleanup`.
      if (cancelled) {
        unlistenAll();
        return;
      }
      cleanup = unlistenAll;
    };

    setupEventListeners();

    return () => {
      cancelled = true;
      cleanup?.();
    };
  }, []);

  // The overlay is its own webview, so it cannot read the menu window's state.
  // It resolves the default critter until #8's persisted `active_critter` lands
  // and both windows read the same setting.
  const { Component: Mascot } = getCritter(DEFAULT_CRITTER_ID);

  // Drive the critter's mic-level visual from the loudest live band -- the frog
  // croaks along with your voice while recording, and rests while transcribing.
  // A critter whose micLevel is "none" ignores this, and nothing else here draws
  // the level, so adding one means deciding what the overlay shows instead.
  // bandsToAmplitude is shared with the menu wordmark's critter (useMicLevel) so
  // the same voice moves both the same way.
  //
  // Reduce-motion is checked here rather than left to RecordingOverlay.css,
  // because the amplitude reaches the critter as an inline style and would win
  // over any stylesheet rule. This window subscribes to `mic-level` directly for
  // its per-band bars, so it does not inherit useMicLevel's own gating.
  const amp =
    state === "recording" && !reduceMotion ? bandsToAmplitude(levels) : 0;

  return (
    <div
      dir={direction}
      data-state={state}
      className={`recording-overlay ${isVisible ? "fade-in" : ""}`}
    >
      <div className="overlay-left">
        <Mascot size={30} sacScale={amp} />
      </div>

      <div className="overlay-middle" role="status" aria-live="polite">
        {deliveryConfirmation !== null ? (
          <span
            className="delivered-indicator"
            title={
              deliveryConfirmation.length > 0
                ? t("overlay.deliveredHint", { target: deliveryConfirmation })
                : t("overlay.deliveredUnknownHint")
            }
          >
            {deliveryConfirmation.length > 0
              ? t("overlay.delivered", { target: deliveryConfirmation })
              : t("overlay.deliveredUnknown")}
          </span>
        ) : (
          <>
            {targetLockIndicator.visible && (
              <button
                type="button"
                className={`target-lock-indicator${
                  targetLockIndicator.tone === "attention" ? " attention" : ""
                }`}
                title={
                  targetLockIndicator.status === "stale"
                    ? t("overlay.lockStaleHint")
                    : t("overlay.lockedToHint", {
                        target:
                          targetLockIndicator.targetName ||
                          t("overlay.lockedToUnknown"),
                      })
                }
                onClick={unlockTarget}
              >
                {targetLockIndicator.status === "stale"
                  ? t("overlay.lockStale")
                  : t("overlay.lockedTo", {
                      target:
                        targetLockIndicator.targetName ||
                        t("overlay.lockedToUnknown"),
                    })}
              </button>
            )}
            {isRaw && (
              <span className="raw-indicator" title={t("overlay.rawHint")}>
                {t("overlay.raw")}
              </span>
            )}
            {state === "recording" && (
              <div className="bars-container" aria-hidden="true">
                {levels.map((v, i) => (
                  <div
                    key={i}
                    className="bar"
                    style={{
                      height: `${Math.min(20, 4 + Math.pow(v, 0.7) * 16)}px`, // Cap at 20px max height
                      transition:
                        "height 60ms ease-out, opacity 120ms ease-out",
                      opacity: Math.max(0.2, v * 1.7), // Minimum opacity for visibility
                    }}
                  />
                ))}
                <span className="sr-only">{t("overlay.recording")}</span>
              </div>
            )}
            {state === "transcribing" && (
              <div className="state-label transcribing-text">
                <span>{t("overlay.transcribing")}</span>
                {!isRaw && (
                  <span className="state-dots" aria-hidden="true">
                    <i />
                    <i />
                    <i />
                  </span>
                )}
              </div>
            )}
            {state === "processing" && (
              <div className="state-label transcribing-text">
                <span>{t("overlay.processing")}</span>
                {!isRaw && (
                  <span className="state-dots" aria-hidden="true">
                    <i />
                    <i />
                    <i />
                  </span>
                )}
              </div>
            )}
          </>
        )}
      </div>

      <div className="overlay-right">
        {state === "recording" && (
          <button
            type="button"
            className="cancel-button"
            aria-label={t("overlay.cancel")}
            onClick={() => {
              commands.cancelOperation();
            }}
          >
            <CancelIcon />
          </button>
        )}
      </div>
    </div>
  );
};

export default RecordingOverlay;
