import React from "react";
import { useTranslation } from "react-i18next";

interface DictionaryGuideProps {
  grouped?: boolean;
}

// Worked examples are keyed so each renders a title + body pair from i18n.
const EXAMPLE_KEYS = ["name", "domain", "handle"] as const;

// Explains how the two dictionary tools (custom words vs word replacements)
// work together, with worked recipes for the cases users hit most: a hard
// name, a domain/email, and a handle. Sits above CustomWords in the
// transcription settings group. See issue #10.
export const DictionaryGuide: React.FC<DictionaryGuideProps> = React.memo(
  ({ grouped = false }) => {
    const { t } = useTranslation();

    return (
      <div
        className={`px-4 py-3 ${grouped ? "" : "rounded-lg border border-mid-gray/20"} flex flex-col gap-2`}
      >
        <h3 className="text-sm font-medium">
          {t("settings.advanced.dictionaryGuide.title")}
        </h3>
        <p className="text-xs text-mid-gray leading-relaxed">
          {t("settings.advanced.dictionaryGuide.intro")}
        </p>
        <p className="text-xs text-mid-gray leading-relaxed">
          {t("settings.advanced.dictionaryGuide.rule")}
        </p>

        <details className="group mt-1">
          <summary className="inline-flex items-center gap-1 cursor-pointer select-none list-none text-xs font-medium text-logo-primary [&::-webkit-details-marker]:hidden">
            <svg
              className="w-3 h-3 transition-transform duration-200 group-open:rotate-90"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth={2}
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M9 5l7 7-7 7"
              />
            </svg>
            {t("settings.advanced.dictionaryGuide.examplesToggle")}
          </summary>

          <div className="mt-2 flex flex-col gap-3 pl-4">
            {EXAMPLE_KEYS.map((key) => (
              <div key={key} className="flex flex-col gap-0.5">
                <div className="text-xs font-medium">
                  {t(`settings.advanced.dictionaryGuide.examples.${key}.title`)}
                </div>
                <div className="text-xs text-mid-gray leading-relaxed">
                  {t(`settings.advanced.dictionaryGuide.examples.${key}.body`)}
                </div>
              </div>
            ))}
            <p className="text-xs text-mid-gray leading-relaxed border-t border-mid-gray/15 pt-2">
              {t("settings.advanced.dictionaryGuide.engineNote")}
            </p>
          </div>
        </details>
      </div>
    );
  },
);
