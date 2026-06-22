import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";
import { UPDATER_FEED_READY } from "../../lib/updater";

interface UpdateChecksToggleProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const UpdateChecksToggle: React.FC<UpdateChecksToggleProps> = ({
  descriptionMode = "tooltip",
  grouped = false,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  // Milestone A: the updater still points at upstream Handy (see
  // src/lib/updater.ts), so the toggle is disabled and shown off. Enabling it
  // would have no effect anyway - UpdateChecker gates on UPDATER_FEED_READY -
  // but a disabled control avoids offering a setting that cannot take effect.
  const updateChecksEnabled =
    UPDATER_FEED_READY && (getSetting("update_checks_enabled") ?? true);

  return (
    <ToggleSwitch
      checked={updateChecksEnabled}
      onChange={(enabled) => updateSetting("update_checks_enabled", enabled)}
      disabled={!UPDATER_FEED_READY}
      isUpdating={isUpdating("update_checks_enabled")}
      label={t("settings.debug.updateChecks.label")}
      description={t("settings.debug.updateChecks.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
    />
  );
};
