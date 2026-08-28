import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import type {
  AutoSubmitKey,
  ClipboardHandling,
  OutputProfile,
  PasteMethod,
} from "@/bindings";
import { useSettings } from "../../hooks/useSettings";
import { useOsType } from "../../hooks/useOsType";
import {
  pasteMethodModifierForOs,
  profilePasteMethodsForOs,
} from "@/lib/paste-methods";
import { Input } from "../ui/Input";
import { Button } from "../ui/Button";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";

interface OutputProfilesProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

// Each profile is looked up on every delivery, so keep the list short enough to
// stay a list a person can read.
const PROFILE_CAP = 50;
const APP_NAME_MAX_LEN = 100;

// The value a dropdown shows when a profile leaves that setting alone. It is not
// a stored value: choosing it writes null, which is how "use the global setting"
// is stored.
const INHERIT = "inherit";

type AutoSubmitChoice = "inherit" | "off" | AutoSubmitKey;

export const OutputProfiles: React.FC<OutputProfilesProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();
    const osType = useOsType();
    const [appName, setAppName] = useState("");

    const profiles = getSetting("output_profiles") || [];
    const updating = isUpdating("output_profiles");

    // The whole list is one setting, so every add, edit, and remove writes it
    // back in full.
    const save = (next: OutputProfile[]) =>
      updateSetting("output_profiles", next);

    const normalize = (name: string) =>
      name
        .trim()
        .toLowerCase()
        .replace(/\.exe$/, "");

    const handleAdd = () => {
      const trimmed = appName.trim();
      if (!trimmed) return;
      if (trimmed.length > APP_NAME_MAX_LEN) {
        toast.error(
          t("settings.advanced.outputProfiles.tooLong", {
            max: APP_NAME_MAX_LEN,
          }),
        );
        return;
      }
      if (profiles.length >= PROFILE_CAP) {
        toast.error(
          t("settings.advanced.outputProfiles.capReached", {
            cap: PROFILE_CAP,
          }),
        );
        return;
      }
      // The backend matches without regard to case or a ".exe" ending, so two
      // entries that differ only that way would be one profile with an
      // unpredictable winner.
      if (profiles.some((p) => normalize(p.app_name) === normalize(trimmed))) {
        toast.error(
          t("settings.advanced.outputProfiles.duplicate", { app: trimmed }),
        );
        return;
      }
      const entry: OutputProfile = {
        app_name: trimmed,
        paste_method: null,
        auto_submit: null,
        auto_submit_key: null,
        clipboard_handling: null,
      };
      save([...profiles, entry]);
      setAppName("");
    };

    const handleRemove = (index: number) => {
      save(profiles.filter((_, i) => i !== index));
    };

    const patch = (index: number, change: Partial<OutputProfile>) => {
      save(profiles.map((p, i) => (i === index ? { ...p, ...change } : p)));
    };

    const handleKeyPress = (e: React.KeyboardEvent) => {
      if (e.key === "Enter") {
        e.preventDefault();
        handleAdd();
      }
    };

    const inheritOption = {
      value: INHERIT,
      label: t("settings.advanced.outputProfiles.useGlobal"),
    };

    const pasteMethodLabels: Record<PasteMethod, string> = {
      ctrl_v: t("settings.advanced.pasteMethod.options.clipboard", {
        modifier: pasteMethodModifierForOs(osType),
      }),
      ctrl_shift_v: t(
        "settings.advanced.pasteMethod.options.clipboardCtrlShiftV",
      ),
      shift_insert: t(
        "settings.advanced.pasteMethod.options.clipboardShiftInsert",
      ),
      direct: t("settings.advanced.pasteMethod.options.direct"),
      none: t("settings.advanced.pasteMethod.options.none"),
      external_script: t(
        "settings.advanced.pasteMethod.options.externalScript",
      ),
    };

    const pasteMethodOptions = [
      inheritOption,
      ...profilePasteMethodsForOs(osType).map((method) => ({
        value: method,
        label: pasteMethodLabels[method],
      })),
    ];

    const autoSubmitOptions = [
      inheritOption,
      { value: "off", label: t("settings.advanced.autoSubmit.options.off") },
      {
        value: "enter",
        label: t("settings.advanced.autoSubmit.options.enter"),
      },
      {
        value: "ctrl_enter",
        label: t("settings.advanced.autoSubmit.options.ctrlEnter"),
      },
      {
        value: "cmd_enter",
        label: t("settings.advanced.autoSubmit.options.superEnter"),
      },
    ];

    const clipboardOptions = [
      inheritOption,
      {
        value: "dont_modify",
        label: t("settings.advanced.clipboardHandling.options.dontModify"),
      },
      {
        value: "copy_to_clipboard",
        label: t("settings.advanced.clipboardHandling.options.copyToClipboard"),
      },
    ];

    const autoSubmitValue = (profile: OutputProfile): AutoSubmitChoice => {
      if (profile.auto_submit === null || profile.auto_submit === undefined) {
        return INHERIT;
      }
      if (!profile.auto_submit) return "off";
      return (profile.auto_submit_key ?? "enter") as AutoSubmitKey;
    };

    const handleAutoSubmitSelect = (index: number, value: string) => {
      const choice = value as AutoSubmitChoice;
      if (choice === INHERIT) {
        patch(index, { auto_submit: null, auto_submit_key: null });
        return;
      }
      if (choice === "off") {
        patch(index, { auto_submit: false, auto_submit_key: null });
        return;
      }
      patch(index, { auto_submit: true, auto_submit_key: choice });
    };

    return (
      <>
        <SettingContainer
          title={t("settings.advanced.outputProfiles.title")}
          description={t("settings.advanced.outputProfiles.description")}
          descriptionMode={descriptionMode}
          grouped={grouped}
          layout="stacked"
        >
          <div className="flex items-center gap-2 w-full">
            <Input
              type="text"
              value={appName}
              onChange={(e) => setAppName(e.target.value)}
              onKeyDown={handleKeyPress}
              placeholder={t(
                "settings.advanced.outputProfiles.appNamePlaceholder",
              )}
              variant="compact"
              className="flex-1 min-w-0"
              disabled={updating}
            />
            <Button
              onClick={handleAdd}
              disabled={!appName.trim() || updating}
              variant="primary"
              size="md"
              className="shrink-0"
            >
              {t("settings.advanced.outputProfiles.add")}
            </Button>
          </div>
        </SettingContainer>

        {profiles.length > 0 && (
          <div
            className={`px-4 p-2 ${grouped ? "" : "rounded-lg border border-mid-gray/20"} flex flex-col gap-3`}
          >
            {profiles.map((profile, index) => (
              <div
                key={`${profile.app_name}-${index}`}
                className="flex flex-col gap-2"
              >
                <div className="flex items-center gap-2">
                  <span className="font-mono text-sm">{profile.app_name}</span>
                  <Button
                    onClick={() => handleRemove(index)}
                    disabled={updating}
                    variant="secondary"
                    size="sm"
                    className="ml-auto inline-flex items-center cursor-pointer"
                    aria-label={t("settings.advanced.outputProfiles.remove", {
                      app: profile.app_name,
                    })}
                  >
                    <svg
                      className="w-3 h-3"
                      fill="none"
                      stroke="currentColor"
                      viewBox="0 0 24 24"
                    >
                      <path
                        strokeLinecap="round"
                        strokeLinejoin="round"
                        strokeWidth={2}
                        d="M6 18L18 6M6 6l12 12"
                      />
                    </svg>
                  </Button>
                </div>
                <div className="grid grid-cols-1 sm:grid-cols-3 gap-2">
                  <label className="flex flex-col gap-1 text-xs text-mid-gray">
                    {t("settings.advanced.outputProfiles.pasteMethod")}
                    <Dropdown
                      options={pasteMethodOptions}
                      selectedValue={profile.paste_method ?? INHERIT}
                      onSelect={(value) =>
                        patch(index, {
                          paste_method:
                            value === INHERIT ? null : (value as PasteMethod),
                        })
                      }
                      disabled={updating}
                    />
                  </label>
                  <label className="flex flex-col gap-1 text-xs text-mid-gray">
                    {t("settings.advanced.outputProfiles.autoSubmit")}
                    <Dropdown
                      options={autoSubmitOptions}
                      selectedValue={autoSubmitValue(profile)}
                      onSelect={(value) => handleAutoSubmitSelect(index, value)}
                      disabled={updating}
                    />
                  </label>
                  <label className="flex flex-col gap-1 text-xs text-mid-gray">
                    {t("settings.advanced.outputProfiles.clipboard")}
                    <Dropdown
                      options={clipboardOptions}
                      selectedValue={profile.clipboard_handling ?? INHERIT}
                      onSelect={(value) =>
                        patch(index, {
                          clipboard_handling:
                            value === INHERIT
                              ? null
                              : (value as ClipboardHandling),
                        })
                      }
                      disabled={updating}
                    />
                  </label>
                </div>
              </div>
            ))}
          </div>
        )}
      </>
    );
  },
);
