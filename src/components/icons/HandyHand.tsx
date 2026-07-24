import { DEFAULT_CRITTER_ID, getCritter } from "./critters";

// Sidebar "General" nav glyph: a calm, static frog face. File name kept to avoid
// import churn; the live/animated frog lives in the wordmark (HandyTextLogo).
const HandyHand = ({
  width,
  height,
  className,
}: {
  width?: number | string;
  height?: number | string;
  className?: string;
}) => {
  // Same registry as the wordmark beside it in the sidebar, so the two never
  // disagree about which critter is active once the picker lands (#8 slice 3).
  const { Component: Mascot } = getCritter(DEFAULT_CRITTER_ID);
  return <Mascot size={width ?? height ?? 24} className={className} />;
};

export default HandyHand;
