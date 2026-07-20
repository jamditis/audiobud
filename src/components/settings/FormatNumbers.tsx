import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

interface FormatNumbersProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const FormatNumbers: React.FC<FormatNumbersProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const enabled = getSetting("format_numbers") ?? true;

    return (
      <ToggleSwitch
        checked={enabled}
        onChange={(value) => updateSetting("format_numbers", value)}
        isUpdating={isUpdating("format_numbers")}
        label={t("settings.advanced.formatNumbers.label")}
        description={t("settings.advanced.formatNumbers.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      />
    );
  },
);
