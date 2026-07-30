import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { SettingContainer } from "../../ui/SettingContainer";
import { Button } from "../../ui/Button";
import { useSettings } from "../../../hooks/useSettings";
import { useOsType } from "../../../hooks/useOsType";
import { formatKeyCombination } from "../../../lib/utils/keyboard";
import type { SidebarSection } from "../../Sidebar";

/**
 * The on-demand guide (issue #14). It is deliberately collapsed until asked
 * for: this is the refresher someone pulls up on purpose, never a first-run
 * popup, so it holds no persisted "seen" state and nothing opens it but the
 * button below.
 *
 * Read-only by design. The onboarding cards it resembles are wired to setup
 * actions (download a model, grant accessibility); reusing them here would drag
 * those side effects into a page that should only explain things. Each card
 * instead offers a jump to the settings section that owns the real control.
 */

/** A card whose `section`, when present, is where the real setting lives.
 * `modeAware` cards describe the record gesture, which differs between
 * push-to-talk and toggle, so their body is chosen from the live setting. */
interface GuideCard {
  key: string;
  section?: SidebarSection;
  modeAware?: boolean;
}

const GUIDE_CARDS: readonly GuideCard[] = [
  { key: "basics", modeAware: true },
  { key: "shortcut", section: "general", modeAware: true },
  { key: "overlay", section: "advanced" },
  { key: "models", section: "models" },
  { key: "customWords", section: "advanced" },
  { key: "limits", section: "history" },
  { key: "privacy" },
] as const;

interface GettingStartedGuideProps {
  /** Jump to the settings section that owns a card's control. Omitted in
   * contexts without a navigator (the cards simply render without the jump). */
  onNavigate?: (section: SidebarSection) => void;
}

export const GettingStartedGuide: React.FC<GettingStartedGuideProps> = ({
  onNavigate,
}) => {
  const { t } = useTranslation();
  const { getSetting } = useSettings();
  const osType = useOsType();
  const [open, setOpen] = useState(false);

  // push_to_talk defaults to false, so the shipped default is toggle, not hold.
  // Telling every reader to "hold the shortcut" would be wrong out of the box,
  // so the gesture copy follows the real setting.
  const pushToTalk = getSetting("push_to_talk") || false;

  // Show the binding the user actually has, not the shipped default: someone
  // who rebound the shortcut is exactly the person a refresher has to be right
  // for. Formatted through the same helper the shortcut settings use so the
  // two screens never disagree about how a combination is spelled.
  const bindings = getSetting("bindings") || {};
  const currentBinding = bindings["transcribe"]?.current_binding;
  const shortcut = currentBinding
    ? formatKeyCombination(currentBinding, osType)
    : t("settings.about.guide.cards.shortcut.unset");

  // The overlay card tells the reader to look for this badge, so it reads the
  // badge's own string rather than repeating the text in 20 guide strings that
  // would then drift the first time the badge is renamed.
  const rawLabel = t("overlay.raw");

  // Titles and bodies get the same values, so a string that interpolates works
  // wherever it lives. i18next renders a missing variable verbatim instead of
  // failing, so splitting these is a defect no gate would catch.
  const cardValues = { shortcut, rawLabel };

  return (
    <>
      <SettingContainer
        title={t("settings.about.guide.title")}
        description={t("settings.about.guide.description")}
        grouped={true}
      >
        <Button
          variant="secondary"
          size="md"
          onClick={() => setOpen((wasOpen) => !wasOpen)}
          aria-expanded={open}
        >
          {open
            ? t("settings.about.guide.hideButton")
            : t("settings.about.guide.showButton")}
        </Button>
      </SettingContainer>

      {open && (
        <div className="flex flex-col gap-3 px-4 pb-4">
          {GUIDE_CARDS.map(({ key, section, modeAware }) => (
            <div
              key={key}
              className="rounded-lg border border-mid-gray/30 bg-mid-gray/5 p-3"
            >
              <div className="text-sm font-semibold">
                {t(`settings.about.guide.cards.${key}.title`, cardValues)}
              </div>
              <p className="mt-1 text-sm text-mid-gray">
                {t(
                  modeAware
                    ? `settings.about.guide.cards.${key}.${pushToTalk ? "bodyHold" : "bodyToggle"}`
                    : `settings.about.guide.cards.${key}.body`,
                  cardValues,
                )}
              </p>
              {section && onNavigate && (
                <button
                  type="button"
                  onClick={() => onNavigate(section)}
                  className="mt-2 text-sm font-semibold text-logo-primary hover:underline cursor-pointer text-start"
                >
                  {t(`settings.about.guide.cards.${key}.jump`)}
                </button>
              )}
            </div>
          ))}
        </div>
      )}
    </>
  );
};
