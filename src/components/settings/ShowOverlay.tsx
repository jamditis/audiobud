import React from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { Button } from "../ui/Button";
import { useSettings } from "../../hooks/useSettings";
import { useOsType } from "../../hooks/useOsType";
import type { OverlayAnchor, OverlayPosition } from "@/bindings";

interface ShowOverlayProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

// Row-major 3x3 anchor grid: top row, middle row, bottom row. Each cell maps to
// an OverlayAnchor and an i18n label used for its accessible name.
const OVERLAY_ANCHORS: { value: OverlayAnchor; labelKey: string }[] = [
  { value: "topleft", labelKey: "settings.advanced.overlay.anchors.topleft" },
  {
    value: "topcenter",
    labelKey: "settings.advanced.overlay.anchors.topcenter",
  },
  { value: "topright", labelKey: "settings.advanced.overlay.anchors.topright" },
  {
    value: "middleleft",
    labelKey: "settings.advanced.overlay.anchors.middleleft",
  },
  {
    value: "middlecenter",
    labelKey: "settings.advanced.overlay.anchors.middlecenter",
  },
  {
    value: "middleright",
    labelKey: "settings.advanced.overlay.anchors.middleright",
  },
  {
    value: "bottomleft",
    labelKey: "settings.advanced.overlay.anchors.bottomleft",
  },
  {
    value: "bottomcenter",
    labelKey: "settings.advanced.overlay.anchors.bottomcenter",
  },
  {
    value: "bottomright",
    labelKey: "settings.advanced.overlay.anchors.bottomright",
  },
];

export const ShowOverlay: React.FC<ShowOverlayProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const {
      getSetting,
      updateSetting,
      isUpdating,
      setOverlayAnchor,
      resetOverlayPosition,
    } = useSettings();

    const overlayOptions = [
      { value: "none", label: t("settings.advanced.overlay.options.none") },
      { value: "bottom", label: t("settings.advanced.overlay.options.bottom") },
      { value: "top", label: t("settings.advanced.overlay.options.top") },
    ];

    const selectedPosition = (getSetting("overlay_position") ||
      "bottom") as OverlayPosition;
    const customPosition = getSetting("overlay_custom_position");
    const updating = isUpdating("overlay_position");

    // The fine grid free-positions the overlay, which Windows and macOS honor
    // via set_position. Linux's GTK layer-shell can only anchor to the Top or
    // Bottom edge, so offering 9 cells there would let users save a placement
    // the overlay silently ignores. Linux keeps the coarse Top/Bottom dropdown.
    const osType = useOsType();
    const supportsFineGrid = osType !== "linux";

    // The grid highlights the saved anchor; with no custom placement it falls
    // back to the centered cell matching the coarse Top/Bottom choice.
    const selectedAnchor: OverlayAnchor =
      customPosition?.anchor ??
      (selectedPosition === "top" ? "topcenter" : "bottomcenter");

    return (
      <>
        <SettingContainer
          title={t("settings.advanced.overlay.title")}
          description={t("settings.advanced.overlay.description")}
          descriptionMode={descriptionMode}
          grouped={grouped}
        >
          <Dropdown
            options={overlayOptions}
            selectedValue={selectedPosition}
            onSelect={(value) =>
              updateSetting("overlay_position", value as OverlayPosition)
            }
            disabled={updating}
          />
        </SettingContainer>

        {selectedPosition !== "none" && supportsFineGrid && (
          <SettingContainer
            title={t("settings.advanced.overlay.fine.title")}
            description={t("settings.advanced.overlay.fine.description")}
            descriptionMode={descriptionMode}
            grouped={grouped}
            layout="stacked"
          >
            <div className="flex items-end gap-4">
              <div
                role="radiogroup"
                aria-label={t("settings.advanced.overlay.fine.title")}
                className="grid aspect-[16/10] w-40 grid-cols-3 gap-1.5 rounded-md border border-mid-gray/30 bg-mid-gray/5 p-1.5"
              >
                {OVERLAY_ANCHORS.map((anchor) => {
                  const isSelected = anchor.value === selectedAnchor;
                  return (
                    <button
                      key={anchor.value}
                      type="button"
                      role="radio"
                      aria-checked={isSelected}
                      aria-label={t(anchor.labelKey)}
                      title={t(anchor.labelKey)}
                      disabled={updating}
                      onClick={() => setOverlayAnchor(anchor.value)}
                      className={`flex items-center justify-center rounded transition-colors ${
                        isSelected
                          ? "border border-logo-primary bg-logo-primary/25"
                          : "border border-transparent hover:border-logo-primary/40 hover:bg-mid-gray/15"
                      } ${updating ? "cursor-not-allowed opacity-50" : "cursor-pointer"}`}
                    >
                      <span
                        className={`h-1.5 w-1.5 rounded-full ${
                          isSelected ? "bg-logo-primary" : "bg-text/30"
                        }`}
                      />
                    </button>
                  );
                })}
              </div>

              {customPosition && (
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={() => resetOverlayPosition()}
                  disabled={updating}
                >
                  {t("settings.advanced.overlay.fine.reset")}
                </Button>
              )}
            </div>
          </SettingContainer>
        )}
      </>
    );
  },
);
