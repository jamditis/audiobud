import React, { useState } from "react";
import { Volume2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useSettingsStore } from "../../stores/settingsStore";
import { Button } from "../ui/Button";
import { SettingContainer } from "../ui/SettingContainer";

interface OutputTestProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  disabled?: boolean;
}

const BARS = [0, 1, 2, 3, 4];

// Output-device check: plays the start+stop feedback sounds through the selected
// output device. There is no system-output loopback level to meter, so the
// bouncing bars indicate active playback rather than a measured signal.
export const OutputTest: React.FC<OutputTestProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false, disabled = false }) => {
    const { t } = useTranslation();
    const playTestSound = useSettingsStore((s) => s.playTestSound);
    const [playing, setPlaying] = useState(false);

    const handleTest = async () => {
      if (playing) return;
      setPlaying(true);
      try {
        await playTestSound("start");
        await playTestSound("stop");
      } catch {
        // Ignore playback errors; the audible result is the real signal.
      } finally {
        setPlaying(false);
      }
    };

    return (
      <SettingContainer
        title={t("settings.sound.outputTest.title")}
        description={t("settings.sound.outputTest.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
        disabled={disabled}
      >
        <div className="flex items-center gap-3">
          <div className="flex items-end gap-[2px] h-5 w-10" aria-hidden="true">
            {BARS.map((i) => (
              <div
                key={i}
                className={`flex-1 rounded-sm bg-logo-primary ${playing ? "eqbar" : ""}`}
                style={{
                  height: "100%",
                  opacity: playing ? 0.9 : 0.2,
                  transform: playing ? undefined : "scaleY(0.25)",
                  transformOrigin: "bottom",
                  animationDelay: playing ? `${i * 0.09}s` : undefined,
                }}
              />
            ))}
          </div>
          <Button
            onClick={handleTest}
            disabled={disabled || playing}
            variant="secondary"
            size="md"
          >
            <span className="inline-flex items-center gap-1">
              <Volume2 className="h-4 w-4" />
              {playing
                ? t("settings.sound.outputTest.playing")
                : t("settings.sound.outputTest.button")}
            </span>
          </Button>
        </div>
      </SettingContainer>
    );
  },
);
