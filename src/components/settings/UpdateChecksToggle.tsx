import React from "react";
import { useTranslation } from "react-i18next";
import { platform } from "@tauri-apps/plugin-os";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";
import { updaterFeedReady } from "../../lib/updater";

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
  const feedReady = updaterFeedReady(platform());
  const updateChecksEnabled =
    feedReady && (getSetting("update_checks_enabled") ?? true);

  if (!feedReady) return null;

  return (
    <ToggleSwitch
      checked={updateChecksEnabled}
      onChange={(enabled) => updateSetting("update_checks_enabled", enabled)}
      disabled={!feedReady}
      isUpdating={isUpdating("update_checks_enabled")}
      label={t("settings.debug.updateChecks.label")}
      description={t("settings.debug.updateChecks.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
    />
  );
};
