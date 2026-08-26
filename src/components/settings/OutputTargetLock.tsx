import React from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../ui/Button";
import { SettingContainer } from "../ui/SettingContainer";
import { useOutputTargetLock } from "../../hooks/useOutputTargetLock";

interface OutputTargetLockProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

/**
 * Shows the output-target lock state (#255) and offers the quick-unlock
 * affordance settings needs to match #121's acceptance criteria: "the target
 * is identifiable without opening settings" implies settings itself must
 * never show something different from the overlay or the tray, and this
 * renders through the same `deriveIndicator` core they do (via
 * `useOutputTargetLock`) so it cannot.
 */
export const OutputTargetLock: React.FC<OutputTargetLockProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { indicator, unlock } = useOutputTargetLock();

    const target =
      indicator.targetName ||
      t("settings.advanced.outputTargetLock.unknownTarget");
    const statusText =
      indicator.status === "locked"
        ? t("settings.advanced.outputTargetLock.lockedStatus", { target })
        : indicator.status === "stale"
          ? t("settings.advanced.outputTargetLock.staleStatus", { target })
          : t("settings.advanced.outputTargetLock.unlockedStatus");

    return (
      <SettingContainer
        title={t("settings.advanced.outputTargetLock.title")}
        description={t("settings.advanced.outputTargetLock.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      >
        <div className="flex items-center gap-3">
          <span
            className={`text-sm ${indicator.tone === "attention" ? "text-amber-500" : "text-mid-gray"}`}
          >
            {statusText}
          </span>
          {indicator.showUnlock && (
            <Button variant="secondary" size="sm" onClick={unlock}>
              {t("settings.advanced.outputTargetLock.unlock")}
            </Button>
          )}
        </div>
      </SettingContainer>
    );
  },
);
