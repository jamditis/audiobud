import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

interface FormatRawOutputProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const FormatRawOutput: React.FC<FormatRawOutputProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const enabled = getSetting("format_raw_output") ?? false;

    return (
      <ToggleSwitch
        checked={enabled}
        onChange={(value) => updateSetting("format_raw_output", value)}
        isUpdating={isUpdating("format_raw_output")}
        label={t("settings.advanced.formatRawOutput.label")}
        description={t("settings.advanced.formatRawOutput.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      />
    );
  },
);
