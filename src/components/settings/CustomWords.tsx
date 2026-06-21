import React, { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { useSettings } from "../../hooks/useSettings";
import { parseWordList, CUSTOM_WORDS_CAP } from "../../lib/wordList";
import { Input } from "../ui/Input";
import { Textarea } from "../ui/Textarea";
import { Button } from "../ui/Button";
import { SettingContainer } from "../ui/SettingContainer";
import { Tooltip } from "../ui/Tooltip";

interface CustomWordsProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

// Small info badge that reveals the expected file format on hover/tap.
const InfoTag: React.FC<{ text: string }> = ({ text }) => {
  const ref = useRef<HTMLButtonElement>(null);
  const [show, setShow] = useState(false);
  return (
    <button
      type="button"
      ref={ref}
      onMouseEnter={() => setShow(true)}
      onMouseLeave={() => setShow(false)}
      onClick={() => setShow((s) => !s)}
      className="text-mid-gray hover:text-logo-primary transition-colors cursor-help"
      aria-label={text}
    >
      <svg
        className="w-4 h-4"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth={2}
      >
        <circle cx="12" cy="12" r="9" />
        <path strokeLinecap="round" d="M12 11v5" />
        <circle cx="12" cy="7.5" r="0.75" fill="currentColor" stroke="none" />
      </svg>
      {show && (
        <Tooltip targetRef={ref} position="top">
          <span className="text-xs leading-snug">{text}</span>
        </Tooltip>
      )}
    </button>
  );
};

export const CustomWords: React.FC<CustomWordsProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();
    const [newWord, setNewWord] = useState("");
    const [pasteText, setPasteText] = useState("");
    const fileInputRef = useRef<HTMLInputElement>(null);
    const customWords = getSetting("custom_words") || [];
    const updating = isUpdating("custom_words");

    const handleAddWord = () => {
      const trimmedWord = newWord.trim();
      const sanitizedWord = trimmedWord.replace(/[<>"'&]/g, "");
      if (
        sanitizedWord &&
        !sanitizedWord.includes(" ") &&
        sanitizedWord.length <= 50
      ) {
        if (customWords.length >= CUSTOM_WORDS_CAP) {
          toast.error(
            t("settings.advanced.customWords.capReached", {
              cap: CUSTOM_WORDS_CAP,
            }),
          );
          return;
        }
        if (customWords.includes(sanitizedWord)) {
          toast.error(
            t("settings.advanced.customWords.duplicate", {
              word: sanitizedWord,
            }),
          );
          return;
        }
        updateSetting("custom_words", [...customWords, sanitizedWord]);
        setNewWord("");
      }
    };

    // Shared by both the file upload and the paste box: parse, merge, and report
    // exactly what landed and what was skipped (no silent truncation).
    const handleImport = (raw: string) => {
      const result = parseWordList(raw, customWords);
      if (result.toAdd.length > 0) {
        updateSetting("custom_words", [...customWords, ...result.toAdd]);
      }

      const parts: string[] = [];
      if (result.duplicateCount > 0) {
        parts.push(
          t("settings.advanced.customWords.import.skippedDuplicates", {
            count: result.duplicateCount,
          }),
        );
      }
      if (result.overCapCount > 0) {
        parts.push(
          t("settings.advanced.customWords.import.skippedCap", {
            count: result.overCapCount,
            cap: CUSTOM_WORDS_CAP,
          }),
        );
      }
      if (result.invalidCount > 0) {
        parts.push(
          t("settings.advanced.customWords.import.skippedInvalid", {
            count: result.invalidCount,
          }),
        );
      }
      const description = parts.length > 0 ? parts.join(", ") : undefined;

      if (result.addedCount > 0) {
        toast.success(
          t("settings.advanced.customWords.import.added", {
            count: result.addedCount,
          }),
          { description },
        );
      } else {
        toast(t("settings.advanced.customWords.import.none"), { description });
      }
    };

    const handleAddPasted = () => {
      if (!pasteText.trim()) return;
      handleImport(pasteText);
      setPasteText("");
    };

    const handleUploadClick = () => fileInputRef.current?.click();

    const handleFileChange = async (
      e: React.ChangeEvent<HTMLInputElement>,
    ) => {
      const file = e.target.files?.[0];
      e.target.value = ""; // let the same file be picked again
      if (!file) return;
      try {
        const text = await file.text();
        handleImport(text);
      } catch {
        toast.error(t("settings.advanced.customWords.import.readError"));
      }
    };

    const handleRemoveWord = (wordToRemove: string) => {
      updateSetting(
        "custom_words",
        customWords.filter((word) => word !== wordToRemove),
      );
    };

    const handleKeyPress = (e: React.KeyboardEvent) => {
      if (e.key === "Enter") {
        e.preventDefault();
        handleAddWord();
      }
    };

    return (
      <>
        <SettingContainer
          title={t("settings.advanced.customWords.title")}
          description={t("settings.advanced.customWords.description")}
          descriptionMode={descriptionMode}
          grouped={grouped}
        >
          <div className="flex items-center gap-2">
            <Input
              type="text"
              className="max-w-40"
              value={newWord}
              onChange={(e) => setNewWord(e.target.value)}
              onKeyDown={handleKeyPress}
              placeholder={t("settings.advanced.customWords.placeholder")}
              variant="compact"
              disabled={updating}
            />
            <Button
              onClick={handleAddWord}
              disabled={
                !newWord.trim() ||
                newWord.includes(" ") ||
                newWord.trim().length > 50 ||
                updating
              }
              variant="primary"
              size="md"
            >
              {t("settings.advanced.customWords.add")}
            </Button>
          </div>
        </SettingContainer>

        <SettingContainer
          title={t("settings.advanced.customWords.import.title")}
          description={t("settings.advanced.customWords.import.description")}
          descriptionMode={descriptionMode}
          grouped={grouped}
          layout="stacked"
        >
          <div className="flex flex-col gap-2 w-full">
            <Textarea
              value={pasteText}
              onChange={(e) => setPasteText(e.target.value)}
              placeholder={t(
                "settings.advanced.customWords.import.pastePlaceholder",
              )}
              variant="compact"
              disabled={updating}
            />
            <div className="flex items-center gap-2">
              <Button
                onClick={handleAddPasted}
                disabled={!pasteText.trim() || updating}
                variant="primary"
                size="md"
              >
                {t("settings.advanced.customWords.import.add")}
              </Button>
              <Button
                onClick={handleUploadClick}
                disabled={updating}
                variant="secondary"
                size="md"
              >
                {t("settings.advanced.customWords.import.upload")}
              </Button>
              <InfoTag
                text={t("settings.advanced.customWords.import.formatHint")}
              />
              <input
                ref={fileInputRef}
                type="file"
                accept=".txt,.csv,text/plain"
                className="hidden"
                onChange={handleFileChange}
              />
            </div>
          </div>
        </SettingContainer>

        {customWords.length > 0 && (
          <div
            className={`px-4 p-2 ${grouped ? "" : "rounded-lg border border-mid-gray/20"} flex flex-col gap-2`}
          >
            <div className="text-xs text-mid-gray">
              {t("settings.advanced.customWords.count", {
                count: customWords.length,
                cap: CUSTOM_WORDS_CAP,
              })}
            </div>
            <div className="flex flex-wrap gap-1">
              {customWords.map((word) => (
                <Button
                  key={word}
                  onClick={() => handleRemoveWord(word)}
                  disabled={updating}
                  variant="secondary"
                  size="sm"
                  className="inline-flex items-center gap-1 cursor-pointer"
                  aria-label={t("settings.advanced.customWords.remove", {
                    word,
                  })}
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
      </>
    );
  },
);
