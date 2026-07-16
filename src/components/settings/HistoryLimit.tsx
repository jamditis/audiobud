import React from "react";
import { useTranslation } from "react-i18next";
import { useSettings } from "../../hooks/useSettings";
import { Input } from "../ui/Input";
import { SettingContainer } from "../ui/SettingContainer";

interface HistoryLimitProps {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
}

export const HistoryLimit: React.FC<HistoryLimitProps> = ({
  descriptionMode = "inline",
  grouped = false,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  const historyLimit = getSetting("history_limit") ?? 5;
  const retentionPeriod = getSetting("recording_retention_period") || "never";

  const handleChange = async (event: React.ChangeEvent<HTMLInputElement>) => {
    const value = parseInt(event.target.value, 10);
    if (!isNaN(value) && value >= 0) {
      updateSetting("history_limit", value);
    }
  };

  return (
    <div>
      <SettingContainer
        title={t("settings.debug.historyLimit.title")}
        description={t("settings.debug.historyLimit.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
        layout="horizontal"
      >
        <div className="flex items-center space-x-2">
          <Input
            type="number"
            min="0"
            max="1000"
            value={historyLimit}
            onChange={handleChange}
            disabled={isUpdating("history_limit")}
            className="w-20"
          />
          <span className="text-sm text-text">
            {t("settings.debug.historyLimit.entries")}
          </span>
        </div>
      </SettingContainer>
      {/* The limit only drives cleanup in "keep latest N" retention mode, so
          only promise count-based trimming there. */}
      {retentionPeriod === "preserve_limit" && (
        <p className="px-4 pb-2 text-xs text-mid-gray">
          {t("settings.debug.historyLimit.trimNote")}
        </p>
      )}
    </div>
  );
};
