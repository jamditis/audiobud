import React from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { Input } from "../ui/Input";
import { useSettings } from "../../hooks/useSettings";
import { useOsType } from "../../hooks/useOsType";
import type { PasteMethod } from "@/bindings";
import {
  pasteMethodModifierForOs,
  pasteMethodsForOs,
} from "@/lib/paste-methods";

interface PasteMethodProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const PasteMethodSetting: React.FC<PasteMethodProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();
    const osType = useOsType();

    const pasteMethodLabels: Record<PasteMethod, string> = {
      ctrl_v: t("settings.advanced.pasteMethod.options.clipboard", {
        modifier: pasteMethodModifierForOs(osType),
      }),
      direct: t("settings.advanced.pasteMethod.options.direct"),
      none: t("settings.advanced.pasteMethod.options.none"),
      ctrl_shift_v: t(
        "settings.advanced.pasteMethod.options.clipboardCtrlShiftV",
      ),
      shift_insert: t(
        "settings.advanced.pasteMethod.options.clipboardShiftInsert",
      ),
      external_script: t(
        "settings.advanced.pasteMethod.options.externalScript",
      ),
    };

    const selectedMethod = (getSetting("paste_method") ||
      "ctrl_v") as PasteMethod;
    const externalScriptPath = getSetting("external_script_path") || "";

    // Arming the external script pops a native confirmation dialog in the
    // backend (a security gate the webview cannot satisfy on its own), so the
    // path is committed on blur/Enter rather than on every keystroke -
    // otherwise typing a path would trigger one modal per character. A local
    // draft holds the in-progress value and resyncs if the persisted value
    // changes (e.g. a rollback after the user declines the dialog).
    const [scriptPathDraft, setScriptPathDraft] =
      React.useState(externalScriptPath);

    React.useEffect(() => {
      setScriptPathDraft(externalScriptPath);
    }, [externalScriptPath]);

    const commitScriptPath = () => {
      if (scriptPathDraft !== externalScriptPath) {
        updateSetting("external_script_path", scriptPathDraft);
      }
    };

    const pasteMethodOptions = pasteMethodsForOs(osType).map((method) => ({
      value: method,
      label: pasteMethodLabels[method],
    }));

    return (
      <SettingContainer
        title={t("settings.advanced.pasteMethod.title")}
        description={t("settings.advanced.pasteMethod.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
        tooltipPosition="bottom"
      >
        <div className="flex flex-col gap-2">
          <Dropdown
            options={pasteMethodOptions}
            selectedValue={selectedMethod}
            onSelect={(value) =>
              updateSetting("paste_method", value as PasteMethod)
            }
            disabled={isUpdating("paste_method")}
          />
          {selectedMethod === "external_script" && (
            <Input
              type="text"
              value={scriptPathDraft}
              onChange={(e) => setScriptPathDraft(e.target.value)}
              onBlur={commitScriptPath}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.currentTarget.blur();
                }
              }}
              placeholder={t(
                "settings.advanced.pasteMethod.externalScriptPlaceholder",
              )}
              disabled={isUpdating("external_script_path")}
            />
          )}
        </div>
      </SettingContainer>
    );
  },
);
