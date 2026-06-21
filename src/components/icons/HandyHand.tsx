import FrogMascot from "./FrogMascot";

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
}) => <FrogMascot size={width ?? height ?? 24} className={className} />;

export default HandyHand;
