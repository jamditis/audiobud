import LiveFrog from "./LiveFrog";

// Brand wordmark rendered as an identifier so the i18next literal-string rule
// (which targets JSX text, not variables) leaves the product name untranslated.
const WORDMARK = "AudioBud";

// AudioBud wordmark: the live frog mascot next to "AudioBud" in Fredoka.
// File name kept to avoid import churn across the sidebar and onboarding.
const HandyTextLogo = ({
  width = 160,
  className = "",
}: {
  width?: number;
  height?: number;
  className?: string;
}) => {
  // Bungee is a wide, heavy display face, so it runs at a smaller ratio than
  // Fredoka did and with tightened tracking to fit the sidebar column.
  const frogSize = Math.round(width * 0.27);
  const fontSize = Math.round(width * 0.125);
  return (
    <div
      className={className}
      style={{
        display: "flex",
        alignItems: "center",
        gap: Math.round(width * 0.045),
      }}
    >
      <LiveFrog size={frogSize} />
      <span
        style={{
          fontFamily: "'Bungee', system-ui, sans-serif",
          fontWeight: 400,
          fontSize,
          letterSpacing: "-0.02em",
          lineHeight: 1,
          whiteSpace: "nowrap",
          color: "var(--color-logo-primary)",
          textShadow:
            "0 1px 0 var(--color-logo-stroke), 0 0 18px rgba(108,192,74,.25)",
        }}
      >
        {WORDMARK}
      </span>
    </div>
  );
};

export default HandyTextLogo;
