import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { join } from "@tauri-apps/api/path";
import { commands } from "@/bindings";
import { SettingContainer } from "../../ui/SettingContainer";

interface DebugPathsProps {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
}

interface DebugPathValues {
  appData: string;
  models: string;
  settings: string;
}

export const DebugPaths: React.FC<DebugPathsProps> = ({
  descriptionMode = "inline",
  grouped = false,
}) => {
  const { t } = useTranslation();
  const [paths, setPaths] = useState<DebugPathValues | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const loadPaths = async () => {
      try {
        const result = await commands.getAppDirPath();
        if (result.status === "error") {
          setError(result.error);
          return;
        }

        const [models, settings] = await Promise.all([
          join(result.data, "models"),
          join(result.data, "settings_store.json"),
        ]);
        setPaths({ appData: result.data, models, settings });
      } catch (err) {
        setError(
          err instanceof Error ? err.message : "Failed to load app directory",
        );
      }
    };

    loadPaths();
  }, []);

  return (
    <SettingContainer
      title={t("settings.about.appDataDirectory.title")}
      description={t("settings.about.appDataDirectory.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
    >
      {error ? (
        <div className="p-3 bg-red-50 border border-red-200 rounded text-xs text-red-600">
          {t("errors.loadDirectory", { error })}
        </div>
      ) : paths ? (
        <div className="text-sm text-gray-600 space-y-2">
          <div>
            <span className="font-medium">
              {t("settings.debug.paths.appData")}
            </span>{" "}
            <span className="font-mono text-xs select-text break-all">
              {paths.appData}
            </span>
          </div>
          <div>
            <span className="font-medium">
              {t("settings.debug.paths.models")}
            </span>{" "}
            <span className="font-mono text-xs select-text break-all">
              {paths.models}
            </span>
          </div>
          <div>
            <span className="font-medium">
              {t("settings.debug.paths.settings")}
            </span>{" "}
            <span className="font-mono text-xs select-text break-all">
              {paths.settings}
            </span>
          </div>
        </div>
      ) : (
        <div className="animate-pulse space-y-2">
          <div className="h-4 bg-gray-100 rounded" />
          <div className="h-4 bg-gray-100 rounded" />
          <div className="h-4 bg-gray-100 rounded" />
        </div>
      )}
    </SettingContainer>
  );
};
