import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import type { WordReplacement } from "@/bindings";
import { useSettings } from "../../hooks/useSettings";
import { Input } from "../ui/Input";
import { Button } from "../ui/Button";
import { SettingContainer } from "../ui/SettingContainer";

interface WordReplacementsProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

// Deterministic find-and-replace runs on every transcription, so keep the list
// bounded; a runaway list would slow each dictation for no real benefit.
const WORD_REPLACEMENTS_CAP = 200;
const FIELD_MAX_LEN = 100;

export const WordReplacements: React.FC<WordReplacementsProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();
    const [from, setFrom] = useState("");
    const [to, setTo] = useState("");
    const [wholeWord, setWholeWord] = useState(true);
    const [caseSensitive, setCaseSensitive] = useState(false);

    const replacements = getSetting("word_replacements") || [];
    const updating = isUpdating("word_replacements");

    const handleAdd = () => {
      const trimmedFrom = from.trim();
      if (!trimmedFrom) return;
      if (trimmedFrom.length > FIELD_MAX_LEN || to.length > FIELD_MAX_LEN) {
        toast.error(
          t("settings.advanced.wordReplacements.tooLong", {
            max: FIELD_MAX_LEN,
          }),
        );
        return;
      }
      if (replacements.length >= WORD_REPLACEMENTS_CAP) {
        toast.error(
          t("settings.advanced.wordReplacements.capReached", {
            cap: WORD_REPLACEMENTS_CAP,
          }),
        );
        return;
      }
      // A new rule duplicates an existing one only when their matching behavior is identical: the
      // same whole-word and case-sensitivity options, and the same `from` under that case rule.
      // Any difference -- whole-word vs partial (`cat` vs `cat` for `category`), case-sensitive vs
      // not, or distinct case-sensitive text like "US" vs "us" -- is a genuinely different rule that
      // the backend applies in order, so it must remain configurable.
      const isDuplicate = replacements.some(
        (r) =>
          r.whole_word === wholeWord &&
          r.case_sensitive === caseSensitive &&
          (caseSensitive
            ? r.from === trimmedFrom
            : r.from.toLowerCase() === trimmedFrom.toLowerCase()),
      );
      if (isDuplicate) {
        toast.error(
          t("settings.advanced.wordReplacements.duplicate", {
            word: trimmedFrom,
          }),
        );
        return;
      }
      const entry: WordReplacement = {
        from: trimmedFrom,
        to: to.trim(),
        whole_word: wholeWord,
        case_sensitive: caseSensitive,
      };
      updateSetting("word_replacements", [...replacements, entry]);
      setFrom("");
      setTo("");
    };

    const handleRemove = (index: number) => {
      updateSetting(
        "word_replacements",
        replacements.filter((_, i) => i !== index),
      );
    };

    const handleKeyPress = (e: React.KeyboardEvent) => {
      if (e.key === "Enter") {
        e.preventDefault();
        handleAdd();
      }
    };

    return (
      <>
        <SettingContainer
          title={t("settings.advanced.wordReplacements.title")}
          description={t("settings.advanced.wordReplacements.description")}
          descriptionMode={descriptionMode}
          grouped={grouped}
          layout="stacked"
        >
          <div className="flex flex-col gap-2 w-full">
            <div className="flex items-center gap-2">
              <Input
                type="text"
                value={from}
                onChange={(e) => setFrom(e.target.value)}
                onKeyDown={handleKeyPress}
                placeholder={t(
                  "settings.advanced.wordReplacements.fromPlaceholder",
                )}
                variant="compact"
                disabled={updating}
              />
              <span className="text-mid-gray select-none">→</span>
              <Input
                type="text"
                value={to}
                onChange={(e) => setTo(e.target.value)}
                onKeyDown={handleKeyPress}
                placeholder={t(
                  "settings.advanced.wordReplacements.toPlaceholder",
                )}
                variant="compact"
                disabled={updating}
              />
              <Button
                onClick={handleAdd}
                disabled={!from.trim() || updating}
                variant="primary"
                size="md"
              >
                {t("settings.advanced.wordReplacements.add")}
              </Button>
            </div>
            <div className="flex items-center gap-4 text-xs text-mid-gray">
              <label className="inline-flex items-center gap-1.5 cursor-pointer">
                <input
                  type="checkbox"
                  className="accent-logo-primary"
                  checked={wholeWord}
                  onChange={(e) => setWholeWord(e.target.checked)}
                  disabled={updating}
                />
                {t("settings.advanced.wordReplacements.wholeWord")}
              </label>
              <label className="inline-flex items-center gap-1.5 cursor-pointer">
                <input
                  type="checkbox"
                  className="accent-logo-primary"
                  checked={caseSensitive}
                  onChange={(e) => setCaseSensitive(e.target.checked)}
                  disabled={updating}
                />
                {t("settings.advanced.wordReplacements.caseSensitive")}
              </label>
            </div>
          </div>
        </SettingContainer>

        {replacements.length > 0 && (
          <div
            className={`px-4 p-2 ${grouped ? "" : "rounded-lg border border-mid-gray/20"} flex flex-col gap-2`}
          >
            <div className="text-xs text-mid-gray">
              {t("settings.advanced.wordReplacements.count", {
                count: replacements.length,
                cap: WORD_REPLACEMENTS_CAP,
              })}
            </div>
            <div className="flex flex-col gap-1">
              {replacements.map((r, index) => (
                <div
                  key={`${r.from}-${index}`}
                  className="flex items-center gap-2 text-sm"
                >
                  <span className="font-mono">{r.from}</span>
                  <span className="text-mid-gray">→</span>
                  <span className="font-mono">
                    {r.to || (
                      <em className="text-mid-gray not-italic opacity-70">
                        {t("settings.advanced.wordReplacements.deletes")}
                      </em>
                    )}
                  </span>
                  {r.whole_word === false && (
                    <span className="text-[10px] uppercase tracking-wide text-mid-gray">
                      {t("settings.advanced.wordReplacements.partialBadge")}
                    </span>
                  )}
                  {r.case_sensitive && (
                    <span className="text-[10px] uppercase tracking-wide text-mid-gray">
                      {t("settings.advanced.wordReplacements.caseBadge")}
                    </span>
                  )}
                  <Button
                    onClick={() => handleRemove(index)}
                    disabled={updating}
                    variant="secondary"
                    size="sm"
                    className="ml-auto inline-flex items-center cursor-pointer"
                    aria-label={t("settings.advanced.wordReplacements.remove", {
                      word: r.from,
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
              ))}
            </div>
          </div>
        )}
      </>
    );
  },
);
