import { listen } from "@tauri-apps/api/event";
import React, { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { CancelIcon } from "../components/icons";
import { DEFAULT_CRITTER_ID, getCritter } from "../components/icons/critters";
import "./RecordingOverlay.css";
import { commands, events } from "../bindings/overlay";
import { usePrefersReducedMotion } from "@/hooks/useMicLevel";
import { useOutputTargetLock } from "@/hooks/useOutputTargetLock";
import i18n, { syncLanguageFromSettings } from "@/i18n";
import {
  formatDeliveredWindowName,
  truncateMiddle,
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
  // Delivered-transcript confirmation (#165). `full` is the untruncated
  // name for the hint (a tooltip has room for the whole thing); `compact` is
  // a middle-truncated form for the chip's own label, which does not -- see
  // `truncateMiddle`. Both are "" when a delivery happened but neither label
  // lookup resolved a name; `null` when there is nothing to show. State (not
  // refs) is read only from the render below; the event listeners read the
  // *ref* siblings further down, since they are set up once and would
  // otherwise see a stale closure.
  const [deliveryConfirmation, setDeliveryConfirmation] = useState<{
    full: string;
    compact: string;
  } | null>(null);
  const confirmationActiveRef = useRef(false);
  const pendingHideRef = useRef(false);
  const confirmationTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(
    null,
  );
  // Whether a *new* dictation is actively recording right now (#279 review
  // round 5). The transcription worker for an older dictation can finish --
  // freeing the coordinator to let the user start recording again -- before
  // that older dictation's own scheduled paste (a focus borrow, a configured
  // paste delay) has actually run and emitted its delivery confirmation. If
  // that confirmation then arrives while this flag is set, it is stale by
  // definition: whatever it names, it is not what the user is looking at
  // right now, a live recording in progress, so the listener below drops it
  // instead of overwriting the new recording's bars with an old "Sent" chip.
  //
  // Deliberately scoped to `state === "recording"` only, not also
  // "transcribing"/"processing" (#279 review round 6): those two states are
  // when a dictation's *own* legitimate confirmation normally arrives, so
  // gating on them would silently break the ordinary single-dictation case.
  // That leaves one known gap unfixed: an older paste completing while a
  // *newer* dictation is itself transcribing or processing (not recording)
  // still passes this guard and shows a stale chip for up to 1.8s. Closing
  // that gap needs the events to carry a dictation sequence to compare
  // against instead of inferring "newer" from overlay state, which is
  // tracked as issue #298 rather than solved with a bigger state guess here.
  const isRecordingActiveRef = useRef(false);
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
        // A new show supersedes any delivery confirmation (#165) and its
        // pending hide left over from the previous dictation (#279 review
        // round 2) -- otherwise the old "Sent" chip would keep masking the
        // new recording state, and its timer would later hide this overlay
        // out from under it. (The backend independently guards its own
        // native hide the same way, but the React-side state needs its own
        // reset regardless.)
        if (confirmationTimeoutRef.current) {
          clearTimeout(confirmationTimeoutRef.current);
          confirmationTimeoutRef.current = null;
        }
        confirmationActiveRef.current = false;
        pendingHideRef.current = false;
        setDeliveryConfirmation(null);
        isRecordingActiveRef.current = payload.state === "recording";
        setState(payload.state);
        setIsRaw(payload.raw);
        setIsVisible(true);
      });

      // Listen for hide-overlay event from Rust
      const unlistenHide = await listen("hide-overlay", () => {
        isRecordingActiveRef.current = false;
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
      //
      // This does not cover `overlay_position: "none"` (#279 review round 3):
      // `show_overlay_state` (src-tauri/src/overlay.rs) never shows the native
      // overlay window in that mode, so setting React state here updates a
      // webview nothing makes visible -- no confirmation surface exists for
      // that combination today. That is a deliberate scope limit, not a
      // silent failure: adding one needs either a new dependency (a native OS
      // notification plugin, not currently in this project) or a UX decision
      // this PR is not the place to make, so it is left to and tracked by
      // issue #274 rather than solved narrowly here.
      const unlistenDelivered = await events.transcriptDeliveredEvent.listen(
        (event) => {
          if (isRecordingActiveRef.current) {
            // A newer dictation is already recording (#279 review round 5):
            // this confirmation belongs to an older one whose paste was still
            // in flight when the new recording started, so it is stale by
            // definition. The event carries no dictation sequence to compare
            // against (see TranscriptDeliveredEvent in src/bindings.ts) --
            // dropping it outright while a new recording is active is the
            // smaller fix, and covers the timing window the review raised
            // (a slow focus borrow or paste delay outlasting the transcription
            // worker) without touching the Rust event shape. It does not cover
            // every ordering -- see the `isRecordingActiveRef` declaration
            // above and issue #298 for the gap that remains and why closing
            // it needs the sequence this event doesn't carry yet.
            return;
          }
          const fullName = formatDeliveredWindowName(
            event.payload.app ?? undefined,
            event.payload.title ?? undefined,
          );
          // Middle-truncated, not `truncateName`'s head-only form (#279
          // review round 4): `formatDeliveredWindowName`'s "title — app"
          // combination commonly shares a long prefix between two windows of
          // the same app ("Google Docs - A" vs "Google Docs - B"), and a
          // head-only cut collapses both to the same compact label -- the
          // opposite of what a confirmation naming *which* window is for. The
          // tail is wider than truncateMiddle's own default so it reliably
          // covers the whole "— app" suffix plus a little of the title right
          // before it, which is where a distinguishing word most often sits.
          //
          // This is still a fixed-length compromise, not a general fix (#279
          // review round 5): a title whose distinguishing part falls outside
          // both the kept head and the kept tail still collapses two chips to
          // the same compact label. The hint above carries the full,
          // untruncated name for exactly that reason; redesigning the chip's
          // own presentation budget is a product question tracked by issue
          // #274, not solved further here.
          const compactName = truncateMiddle(fullName, 6, 10);
          confirmationActiveRef.current = true;
          pendingHideRef.current = false;
          setDeliveryConfirmation({ full: fullName, compact: compactName });
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
              // The hint gets the full, untruncated name (#279 review round
              // 4) -- a tooltip has the room the compact chip label does not.
              deliveryConfirmation.full.length > 0
                ? t("overlay.deliveredHint", {
                    target: deliveryConfirmation.full,
                  })
                : t("overlay.deliveredUnknownHint")
            }
          >
            {deliveryConfirmation.compact.length > 0
              ? t("overlay.delivered", { target: deliveryConfirmation.compact })
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
