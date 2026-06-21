import { useEffect, useRef, useState, type CSSProperties } from "react";
import FrogMascot from "./icons/FrogMascot";

// Full-bleed wetland atmosphere behind the app: a pond-depth gradient, canopy
// god-rays, drifting mist, fireflies at night, lily pads and reeds at the
// waterline -- plus the konami-code "frog rain" easter egg. Decorative only.
const KONAMI = [38, 38, 40, 40, 37, 39, 37, 39, 66, 65];

interface RainFrog {
  id: number;
  left: number;
  size: number;
  rot: number;
  delay: number;
}

const SwampBackground = () => {
  const firefliesRef = useRef<HTMLDivElement>(null);
  const [rain, setRain] = useState<RainFrog[]>([]);

  // Scatter fireflies once on mount.
  useEffect(() => {
    const wrap = firefliesRef.current;
    if (!wrap) return;
    for (let i = 0; i < 22; i++) {
      const f = document.createElement("div");
      f.className = "ff";
      f.style.left = Math.random() * 100 + "%";
      f.style.top = Math.random() * 100 + "%";
      f.style.animationDuration = `${6 + Math.random() * 8}s, ${1.6 + Math.random() * 2}s`;
      f.style.animationDelay = `${Math.random() * 6}s`;
      wrap.appendChild(f);
    }
    // A friendly hello in the devtools console.
    console.log(
      "%cribbit. AudioBud is listening from the pond -- all local, all swamp.",
      "color:#84d150;font-weight:bold",
    );
  }, []);

  // Konami code -> make it rain frogs.
  useEffect(() => {
    let pos = 0;
    const onKey = (e: KeyboardEvent) => {
      pos = e.keyCode === KONAMI[pos] ? pos + 1 : 0;
      if (pos === KONAMI.length) {
        pos = 0;
        const frogs: RainFrog[] = Array.from({ length: 24 }, (_, i) => ({
          id: Date.now() + i,
          left: Math.random() * 100,
          size: 20 + Math.random() * 28,
          rot: Math.random() * 720 - 360,
          delay: i * 0.07,
        }));
        setRain(frogs);
        setTimeout(() => setRain([]), 4200);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <div
      className="fixed inset-0 -z-10 overflow-hidden pointer-events-none"
      aria-hidden="true"
    >
      <div className="swamp-base" />
      <div className="swamp-rays" />
      <div className="swamp-mist" />
      <div className="swamp-fireflies" ref={firefliesRef} />

      {/* lily pads at the waterline */}
      <svg
        className="swamp-pads"
        viewBox="0 0 1200 160"
        preserveAspectRatio="none"
        xmlns="http://www.w3.org/2000/svg"
      >
        <ellipse cx="160" cy="130" rx="120" ry="34" fill="#1c3a22" />
        <path d="M160 96 L196 130 L124 130 Z" fill="#0c130e" opacity=".6" />
        <ellipse cx="980" cy="140" rx="150" ry="38" fill="#173019" />
        <path d="M980 102 L1020 140 L940 140 Z" fill="#0c130e" opacity=".6" />
        <ellipse cx="560" cy="150" rx="100" ry="26" fill="#15281a" />
      </svg>

      {/* reeds + cattails at each edge */}
      <svg
        className="swamp-reeds"
        style={{ left: -10, bottom: 0, opacity: 0.5 }}
        width="120"
        height="260"
        viewBox="0 0 120 260"
        xmlns="http://www.w3.org/2000/svg"
      >
        <path d="M30 260 C20 160 26 90 18 20" stroke="#2c5234" strokeWidth="5" fill="none" />
        <path d="M58 260 C52 150 60 70 70 14" stroke="#365f3c" strokeWidth="5" fill="none" />
        <rect x="60" y="6" width="14" height="34" rx="7" fill="#7a5a2a" />
        <rect x="10" y="14" width="13" height="30" rx="6" fill="#8a6630" />
      </svg>
      <svg
        className="swamp-reeds"
        style={{ right: -10, bottom: 0, opacity: 0.5, transform: "scaleX(-1)" }}
        width="120"
        height="240"
        viewBox="0 0 120 260"
        xmlns="http://www.w3.org/2000/svg"
      >
        <path d="M30 260 C20 160 26 90 18 20" stroke="#2c5234" strokeWidth="5" fill="none" />
        <path d="M58 260 C52 150 60 70 70 14" stroke="#365f3c" strokeWidth="5" fill="none" />
        <rect x="60" y="6" width="14" height="34" rx="7" fill="#7a5a2a" />
      </svg>

      {/* konami frog rain */}
      {rain.map((f) => (
        <span
          key={f.id}
          className="frog-rain"
          style={
            {
              left: `${f.left}vw`,
              animationDelay: `${f.delay}s`,
              ["--rot" as string]: `${f.rot}deg`,
            } as CSSProperties
          }
        >
          <FrogMascot size={f.size} />
        </span>
      ))}
    </div>
  );
};

export default SwampBackground;
