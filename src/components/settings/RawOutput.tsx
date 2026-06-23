import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

interface RawOutputProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const RawOutput: React.FC<RawOutputProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const enabled = getSetting("raw_output") ?? false;

    return (
      <ToggleSwitch
        checked={enabled}
        onChange={(value) => updateSetting("raw_output", value)}
        isUpdating={isUpdating("raw_output")}
        label={t("settings.advanced.rawOutput.label")}
        description={t("settings.advanced.rawOutput.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      />
    );
  },
);
