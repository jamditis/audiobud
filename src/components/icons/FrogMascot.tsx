import { useId } from "react";
import "./FrogMascot.css";

// AudioBud's red-eyed tree frog mascot. Single source of truth for the wordmark,
// the sidebar nav icon, the recording overlay, and the konami easter egg.
//
// Animatable parts are exposed through props so callers drive state:
//  - blink/wink/croak/lick toggle facial expressions (CSS classes in App.css)
//  - sacScale (0..1) inflates the vocal sac directly, e.g. from live mic level
//  - irisDX/irisDY translate the pupils to follow the cursor
export interface FrogMascotProps {
  size?: number | string;
  className?: string;
  blink?: boolean;
  wink?: boolean;
  croak?: boolean;
  lick?: boolean;
  /** 0 = deflated/hidden, 1 = fully inflated. Overrides the croak class when set. */
  sacScale?: number;
  irisDX?: number;
  irisDY?: number;
}

const FrogMascot = ({
  size,
  className = "",
  blink = false,
  wink = false,
  croak = false,
  lick = false,
  sacScale,
  irisDX = 0,
  irisDY = 0,
}: FrogMascotProps) => {
  const uid = useId().replace(/:/g, "");
  const fb = `fb-${uid}`;
  const sg = `sg-${uid}`;

  const stateClass = [
    "frog",
    blink && "blink",
    wink && "winkL",
    croak && sacScale === undefined && "croak",
    lick && "lick",
    className,
  ]
    .filter(Boolean)
    .join(" ");

  // When a caller drives the sac directly (mic level), bypass the CSS class.
  const sacStyle =
    sacScale === undefined
      ? undefined
      : {
          transform: `scale(${0.2 + sacScale * 0.8})`,
          opacity: sacScale > 0.02 ? 1 : 0,
        };

  const irisStyle = { transform: `translate(${irisDX}px, ${irisDY}px)` };

  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 200 200"
      className={stateClass}
      xmlns="http://www.w3.org/2000/svg"
    >
      <defs>
        <radialGradient id={fb} cx="50%" cy="36%" r="68%">
          <stop offset="0" stopColor="#a6e570" />
          <stop offset="1" stopColor="#56a52d" />
        </radialGradient>
        <linearGradient id={sg} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#cdf293" />
          <stop offset="1" stopColor="#9bd860" />
        </linearGradient>
      </defs>

      {/* vocal sac (croak / mic level) */}
      <g className="sac" style={sacStyle}>
        <ellipse cx="100" cy="152" rx="44" ry="33" fill={`url(#${sg})`} />
        <path
          d="M64 150 q36 26 72 0"
          fill="none"
          stroke="#7cc34a"
          strokeWidth="2.5"
          opacity=".5"
        />
      </g>

      {/* webbed feet */}
      <g>
        <ellipse cx="70" cy="164" rx="22" ry="11" fill="#4f9a2a" />
        <ellipse cx="56" cy="160" rx="7" ry="9" fill="#ff9d3c" />
        <ellipse cx="70" cy="157" rx="7" ry="9" fill="#ff9d3c" />
        <ellipse cx="84" cy="160" rx="7" ry="9" fill="#ff9d3c" />
        <ellipse cx="130" cy="164" rx="22" ry="11" fill="#4f9a2a" />
        <ellipse cx="116" cy="160" rx="7" ry="9" fill="#ff9d3c" />
        <ellipse cx="130" cy="157" rx="7" ry="9" fill="#ff9d3c" />
        <ellipse cx="144" cy="160" rx="7" ry="9" fill="#ff9d3c" />
      </g>

      {/* head + belly + cheeks */}
      <ellipse cx="100" cy="108" rx="62" ry="52" fill={`url(#${fb})`} />
      <ellipse cx="100" cy="128" rx="40" ry="26" fill="#c2ea96" opacity=".30" />
      <ellipse cx="54" cy="120" rx="11" ry="7" fill="#ff8a82" opacity=".35" />
      <ellipse cx="146" cy="120" rx="11" ry="7" fill="#ff8a82" opacity=".35" />

      {/* eye bulges */}
      <circle cx="64" cy="56" r="32" fill={`url(#${fb})`} />
      <circle cx="136" cy="56" r="32" fill={`url(#${fb})`} />

      {/* eyes: white sclera, gold-rimmed red iris, vertical pupil, highlight */}
      <g className="eyeL">
        <circle cx="64" cy="56" r="21" fill="#fff" />
        <g className="iris" style={irisStyle}>
          <circle cx="64" cy="57" r="15" fill="#ff5147" />
          <circle cx="64" cy="57" r="15" fill="none" stroke="#ffd24a" strokeWidth="2" />
          <ellipse cx="64" cy="57" rx="3.4" ry="11" fill="#160c06" />
          <circle cx="59" cy="50" r="3.4" fill="#fff" />
        </g>
      </g>
      <g className="eyeR">
        <circle cx="136" cy="56" r="21" fill="#fff" />
        <g className="iris" style={irisStyle}>
          <circle cx="136" cy="57" r="15" fill="#ff5147" />
          <circle cx="136" cy="57" r="15" fill="none" stroke="#ffd24a" strokeWidth="2" />
          <ellipse cx="136" cy="57" rx="3.4" ry="11" fill="#160c06" />
          <circle cx="131" cy="50" r="3.4" fill="#fff" />
        </g>
      </g>

      {/* nostrils */}
      <circle cx="92" cy="100" r="2.8" fill="#2b5121" />
      <circle cx="108" cy="100" r="2.8" fill="#2b5121" />

      {/* mouth: closed smile / open croak */}
      <path
        className="mouthClosed"
        d="M60 122 Q100 156 140 122"
        fill="none"
        stroke="#2b5121"
        strokeWidth="5"
        strokeLinecap="round"
      />
      <ellipse className="mouthOpen" cx="100" cy="130" rx="19" ry="13" fill="#3a1f14" />

      {/* tongue flick + fly */}
      <g className="tongue">
        <path
          d="M100 148 q34 8 56 -8"
          fill="none"
          stroke="#ff5d77"
          strokeWidth="12"
          strokeLinecap="round"
        />
        <circle cx="160" cy="138" r="6" fill="#2b2b2b" />
      </g>
    </svg>
  );
};

export default FrogMascot;
