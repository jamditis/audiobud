import React, { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useTranslation } from "react-i18next";
import { commands } from "@/bindings";
import { SettingContainer } from "../ui/SettingContainer";

const BAR_COUNT = 16;

interface InputLevelMeterProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

// Live microphone meter for the settings screen. Opens a monitor-only mic
// stream while mounted (no recording) and animates the 16 spectrum bands the
// backend already emits on "mic-level".
export const InputLevelMeter: React.FC<InputLevelMeterProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const [levels, setLevels] = useState<number[]>(() =>
      Array(BAR_COUNT).fill(0),
    );
    const smoothedRef = useRef<number[]>(Array(BAR_COUNT).fill(0));

    useEffect(() => {
      let unlisten: (() => void) | undefined;
      let cancelled = false;

      const start = async () => {
        try {
          await commands.setMicMonitor(true);
          if (cancelled) {
            // Unmounted mid-await: undo the monitor we just enabled.
            await commands.setMicMonitor(false);
            return;
          }
          unlisten = await listen<number[]>("mic-level", (event) => {
            const incoming = event.payload ?? [];
            // Fast attack, slow release for a natural meter feel.
            const next = smoothedRef.current.map((prev, i) => {
              const target = Math.max(0, Math.min(1, incoming[i] ?? 0));
              const a = target > prev ? 0.4 : 0.75;
              return prev * a + target * (1 - a);
            });
            smoothedRef.current = next;
            setLevels([...next]);
          });
        } catch {
          // No mic / monitor unavailable: leave the meter at rest.
        }
      };

      start();

      return () => {
        cancelled = true;
        if (unlisten) unlisten();
        commands.setMicMonitor(false).catch(() => {});
      };
    }, []);

    return (
      <SettingContainer
        title={t("settings.sound.inputLevel.title")}
        description={t("settings.sound.inputLevel.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      >
        <div className="flex items-end gap-[2px] h-6 w-40" aria-hidden="true">
          {levels.map((v, i) => {
            const heightPx = 3 + Math.pow(v, 0.7) * 21; // cap ~24px
            return (
              <div
                key={i}
                className="flex-1 rounded-sm bg-logo-primary"
                style={{
                  height: `${heightPx}px`,
                  opacity: Math.max(0.18, v * 1.6),
                  transition: "height 60ms ease-out, opacity 120ms ease-out",
                }}
              />
            );
          })}
        </div>
      </SettingContainer>
    );
  },
);
