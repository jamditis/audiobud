import { listen } from "@tauri-apps/api/event";
import React, { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { CancelIcon } from "../components/icons";
import FrogMascot from "../components/icons/FrogMascot";
import "./RecordingOverlay.css";
import { commands } from "@/bindings";
import i18n, { syncLanguageFromSettings } from "@/i18n";
import { getLanguageDirection } from "@/lib/utils/rtl";

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
  const direction = getLanguageDirection(i18n.language);

  useEffect(() => {
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
        setIsVisible(false);
        setIsRaw(false);
      });

      // Listen for mic-level updates
      const unlistenLevel = await listen<number[]>("mic-level", (event) => {
        const newLevels = event.payload as number[];

        // Apply smoothing to reduce jitter
        const smoothed = smoothedLevelsRef.current.map((prev, i) => {
          const target = newLevels[i] || 0;
          return prev * 0.7 + target * 0.3; // Smooth transition
        });

        smoothedLevelsRef.current = smoothed;
        setLevels(smoothed.slice(0, 9));
      });

      // Cleanup function
      return () => {
        unlistenShow();
        unlistenHide();
        unlistenLevel();
      };
    };

    setupEventListeners();
  }, []);

  // Drive the frog's vocal sac from the loudest live mic band -- he croaks
  // along with your voice while recording, and rests while transcribing.
  const amp =
    state === "recording" && levels.length
      ? Math.min(1, Math.max(0, ...levels) * 1.4)
      : 0;

  return (
    <div
      dir={direction}
      data-state={state}
      className={`recording-overlay ${isVisible ? "fade-in" : ""}`}
    >
      <div className="overlay-left">
        <FrogMascot size={30} sacScale={amp} />
      </div>

      <div className="overlay-middle" role="status" aria-live="polite">
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
                  transition: "height 60ms ease-out, opacity 120ms ease-out",
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
