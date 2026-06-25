import React, { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { ask, save } from "@tauri-apps/plugin-dialog";
import { commands, type WordSuggestion } from "@/bindings";
import { hasStoredPersonalizationData } from "@/lib/personalization";
import { useSettings } from "../../hooks/useSettings";
import { Button } from "../ui/Button";
import { SettingContainer } from "../ui/SettingContainer";
import { ToggleSwitch } from "../ui/ToggleSwitch";

interface PersonalizationSettingsProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

// How many history-mined suggestions to surface at once.
const SUGGESTION_LIMIT = 20;

/**
 * Opt-in, on-device personalization (issue #16, Tier 1). Off by default. When enabled it mines the
 * user's transcript history for frequently-dictated proper nouns and offers them as one-tap custom
 * words; accepted words bias transcription exactly like the manual dictionary. All data stays on the
 * device and can be exported or reset here.
 */
export const PersonalizationSettings: React.FC<PersonalizationSettingsProps> =
  React.memo(({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, refreshSettings } = useSettings();

    const personalization = getSetting("personalization");
    const enabled = personalization?.enabled ?? false;
    const learnedWords = personalization?.learned_words ?? [];
    const hasStoredData = hasStoredPersonalizationData(personalization);

    const [busy, setBusy] = useState(false);
    const [suggestions, setSuggestions] = useState<WordSuggestion[]>([]);
    const [loadingSuggestions, setLoadingSuggestions] = useState(false);
    // Serializes the single-word mutations (accept / dismiss / remove). Each backend command does a
    // read-modify-write of `personalization`, so two running in parallel could clobber each other's
    // write and lose a word. The buttons are disabled while one is in flight.
    const [mutating, setMutating] = useState(false);

    const loadSuggestions = useCallback(async () => {
      setLoadingSuggestions(true);
      try {
        const res = await commands.getWordSuggestions(SUGGESTION_LIMIT);
        if (res.status === "ok") {
          setSuggestions(res.data);
        } else {
          console.error("Failed to load suggestions:", res.error);
          setSuggestions([]);
        }
      } catch (error) {
        console.error("Failed to load suggestions:", error);
        setSuggestions([]);
      } finally {
        setLoadingSuggestions(false);
      }
    }, []);

    // Refresh suggestions whenever personalization is enabled; clear them when disabled.
    useEffect(() => {
      if (enabled) {
        loadSuggestions();
      } else {
        setSuggestions([]);
      }
    }, [enabled, loadSuggestions]);

    const handleToggle = async (next: boolean) => {
      setBusy(true);
      try {
        const res = await commands.updatePersonalizationEnabled(next);
        if (res.status === "error") throw new Error(res.error);
        await refreshSettings();
      } catch (error) {
        console.error("Failed to toggle personalization:", error);
        toast.error(t("settings.advanced.personalization.error"));
      } finally {
        setBusy(false);
      }
    };

    const handleAccept = async (word: string) => {
      if (mutating) return;
      setMutating(true);
      try {
        const res = await commands.acceptWordSuggestion(word);
        if (res.status === "error") throw new Error(res.error);
        setSuggestions((prev) => prev.filter((s) => s.word !== word));
        await refreshSettings();
      } catch (error) {
        console.error("Failed to accept suggestion:", error);
        toast.error(t("settings.advanced.personalization.error"));
      } finally {
        setMutating(false);
      }
    };

    const handleDismiss = async (word: string) => {
      if (mutating) return;
      setMutating(true);
      try {
        const res = await commands.dismissWordSuggestion(word);
        if (res.status === "error") throw new Error(res.error);
        setSuggestions((prev) => prev.filter((s) => s.word !== word));
      } catch (error) {
        console.error("Failed to dismiss suggestion:", error);
        toast.error(t("settings.advanced.personalization.error"));
      } finally {
        setMutating(false);
      }
    };

    const handleRemoveLearned = async (word: string) => {
      // Removal rebuilds the list from the current `learnedWords` snapshot, so a second call before
      // the first refresh lands would recompute from stale state and could restore a just-removed
      // word. The shared `mutating` guard serializes it with accept/dismiss, so each runs against
      // fresh post-refresh state.
      if (mutating) return;
      setMutating(true);
      try {
        const res = await commands.updateLearnedWords(
          learnedWords.filter((w) => w !== word),
        );
        if (res.status === "error") throw new Error(res.error);
        await refreshSettings();
      } catch (error) {
        console.error("Failed to remove learned word:", error);
        toast.error(t("settings.advanced.personalization.error"));
      } finally {
        setMutating(false);
      }
    };

    const handleReset = async () => {
      const confirmed = await ask(
        t("settings.advanced.personalization.data.resetConfirm"),
        {
          title: t("settings.advanced.personalization.data.resetTitle"),
          kind: "warning",
        },
      );
      if (!confirmed) return;

      setBusy(true);
      try {
        const res = await commands.resetPersonalization();
        if (res.status === "error") throw new Error(res.error);
        await refreshSettings();
        // Only re-mine if the feature is on; resetting while off must not trigger mining (#53).
        if (enabled) {
          await loadSuggestions();
        }
        toast.success(t("settings.advanced.personalization.data.resetDone"));
      } catch (error) {
        console.error("Failed to reset personalization:", error);
        toast.error(t("settings.advanced.personalization.error"));
      } finally {
        setBusy(false);
      }
    };

    const handleExport = async () => {
      try {
        const path = await save({
          defaultPath: "audiobud-personalization.json",
          filters: [{ name: "JSON", extensions: ["json"] }],
        });
        if (!path) return;
        const res = await commands.exportPersonalization(path);
        if (res.status === "error") throw new Error(res.error);
        toast.success(t("settings.advanced.personalization.data.exportDone"));
      } catch (error) {
        console.error("Failed to export personalization:", error);
        toast.error(t("settings.advanced.personalization.data.exportError"));
      }
    };

    return (
      <>
        <ToggleSwitch
          checked={enabled}
          onChange={handleToggle}
          isUpdating={busy}
          label={t("settings.advanced.personalization.title")}
          description={t("settings.advanced.personalization.description")}
          descriptionMode={descriptionMode}
          grouped={grouped}
        />

        {enabled && (
          <SettingContainer
            title={t("settings.advanced.personalization.suggestions.title")}
            description={t(
              "settings.advanced.personalization.suggestions.description",
            )}
            descriptionMode={descriptionMode}
            grouped={grouped}
            layout="stacked"
          >
            <div className="flex flex-col gap-2 w-full">
              <div>
                <Button
                  onClick={loadSuggestions}
                  disabled={loadingSuggestions}
                  variant="secondary"
                  size="sm"
                >
                  {loadingSuggestions
                    ? t("settings.advanced.personalization.suggestions.loading")
                    : t(
                        "settings.advanced.personalization.suggestions.refresh",
                      )}
                </Button>
              </div>
              {suggestions.length === 0 ? (
                <p className="text-xs text-mid-gray">
                  {t("settings.advanced.personalization.suggestions.empty")}
                </p>
              ) : (
                <div className="flex flex-col gap-1">
                  {suggestions.map((s) => (
                    <div
                      key={s.word}
                      className="flex items-center gap-2 text-sm"
                    >
                      <span className="font-mono">{s.word}</span>
                      <span className="text-xs text-mid-gray">
                        {t(
                          "settings.advanced.personalization.suggestions.count",
                          { count: s.count },
                        )}
                      </span>
                      <div className="ml-auto flex items-center gap-1">
                        <Button
                          onClick={() => handleAccept(s.word)}
                          disabled={mutating}
                          variant="primary"
                          size="sm"
                        >
                          {t(
                            "settings.advanced.personalization.suggestions.accept",
                          )}
                        </Button>
                        <Button
                          onClick={() => handleDismiss(s.word)}
                          disabled={mutating}
                          variant="ghost"
                          size="sm"
                          aria-label={t(
                            "settings.advanced.personalization.suggestions.dismissLabel",
                            { word: s.word },
                          )}
                        >
                          {t(
                            "settings.advanced.personalization.suggestions.dismiss",
                          )}
                        </Button>
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>
          </SettingContainer>
        )}

        {learnedWords.length > 0 && (
          <div
            className={`px-4 p-2 ${grouped ? "" : "rounded-lg border border-mid-gray/20"} flex flex-col gap-2`}
          >
            <div className="text-xs text-mid-gray">
              {t("settings.advanced.personalization.learned.count", {
                count: learnedWords.length,
              })}
            </div>
            <div className="flex flex-wrap gap-1">
              {learnedWords.map((word) => (
                <Button
                  key={word}
                  onClick={() => handleRemoveLearned(word)}
                  disabled={mutating}
                  variant="secondary"
                  size="sm"
                  className="inline-flex items-center gap-1 cursor-pointer"
                  aria-label={t(
                    "settings.advanced.personalization.learned.remove",
                    { word },
                  )}
                >
                  <span>{word}</span>
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
              ))}
            </div>
          </div>
        )}

        {hasStoredData && (
          <SettingContainer
            title={t("settings.advanced.personalization.data.title")}
            description={t(
              "settings.advanced.personalization.data.description",
            )}
            descriptionMode={descriptionMode}
            grouped={grouped}
            layout="stacked"
          >
            <div className="flex items-center gap-2">
              <Button
                onClick={handleExport}
                variant="secondary"
                size="md"
                disabled={busy}
              >
                {t("settings.advanced.personalization.data.export")}
              </Button>
              <Button
                onClick={handleReset}
                variant="danger"
                size="md"
                disabled={busy}
              >
                {t("settings.advanced.personalization.data.reset")}
              </Button>
            </div>
          </SettingContainer>
        )}
      </>
    );
  });
