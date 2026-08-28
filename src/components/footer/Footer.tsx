import React, { useState, useEffect } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { openUrl } from "@tauri-apps/plugin-opener";
import { platform } from "@tauri-apps/plugin-os";
import { useTranslation } from "react-i18next";

import { useUpdateChannelAvailable } from "../../hooks/useUpdateChannelAvailable";
import { RELEASES_URL, updaterFeedReady } from "../../lib/updater";
import ModelSelector from "../model-selector";
import UpdateChecker from "../update-checker";

const Footer: React.FC = () => {
  const { t } = useTranslation();
  const [version, setVersion] = useState("");
  const currentPlatform = platform();
  const updateChannelAvailable = useUpdateChannelAvailable();
  const hasAutomaticUpdater =
    updaterFeedReady(currentPlatform) && updateChannelAvailable;
  const hasManualMacUpdate = currentPlatform === "macos";
  const hasReleaseAction = hasAutomaticUpdater || hasManualMacUpdate;

  useEffect(() => {
    const fetchVersion = async () => {
      try {
        const appVersion = await getVersion();
        setVersion(appVersion);
      } catch (error) {
        console.error("Failed to get app version:", error);
        setVersion("");
      }
    };

    fetchVersion();
  }, []);

  return (
    <footer className="app-footer swamp-waterline w-full pt-3">
      <div className="flex justify-between items-center text-xs px-4 pb-3 text-text/60">
        <div className="flex items-center gap-4">
          <ModelSelector />
        </div>

        <div className="flex items-center gap-1">
          {hasAutomaticUpdater && (
            <UpdateChecker updateChannelAvailable={updateChannelAvailable} />
          )}
          {hasManualMacUpdate && (
            <button
              type="button"
              className="transition-colors text-text/60 hover:text-text/80"
              onClick={() => void openUrl(RELEASES_URL)}
            >
              {t("footer.portableUpdateButton")}
            </button>
          )}
          {hasReleaseAction && version && <span aria-hidden="true">•</span>}
          {version && (
            // eslint-disable-next-line i18next/no-literal-string
            <span>v{version}</span>
          )}
        </div>
      </div>
    </footer>
  );
};

export default Footer;
