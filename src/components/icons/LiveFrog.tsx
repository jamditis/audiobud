import { useEffect, useRef, useState } from "react";
import { getCritter } from "./critters";
import { playRibbit } from "../../lib/ribbit";
import { useMicLevel } from "@/hooks/useMicLevel";

// The "alive" layer for any critter: it blinks on its own, its eyes follow the
// cursor, it croaks when clicked, and it answers your voice while you dictate.
// Used for the wordmark and the sidebar nav icon. The blink cadence, cursor-follow
// math, and mic wiring live here once, so a new critter is an SVG plus a registry
// entry rather than a reimplementation of all three.
interface LiveFrogProps {
  size?: number | string;
  className?: string;
  follow?: boolean;
  idleBlink?: boolean;
  clickCroak?: boolean;
  /**
   * Animate the critter's mic-level visual from live input while dictating (and
   * while the settings mic monitor is open). Rests when no level is flowing, and
   * stays at rest under prefers-reduced-motion.
   */
  micLevel?: boolean;
  /** Which critter to render. Falls back to the default for an unknown id. */
  critter?: string;
}

const LiveFrog = ({
  size,
  className,
  follow = true,
  idleBlink = true,
  clickCroak = true,
  micLevel = true,
  critter,
}: LiveFrogProps) => {
  const { Component: Mascot } = getCritter(critter);
  const amp = useMicLevel(micLevel);
  const ref = useRef<HTMLSpanElement>(null);
  const [blink, setBlink] = useState(false);
  const [croak, setCroak] = useState(false);
  const [iris, setIris] = useState({ x: 0, y: 0 });

  // Idle blink at a relaxed, slightly irregular cadence.
  useEffect(() => {
    if (!idleBlink) return;
    let timer: ReturnType<typeof setTimeout>;
    const schedule = () => {
      timer = setTimeout(
        () => {
          setBlink(true);
          setTimeout(() => setBlink(false), 130);
          schedule();
        },
        3000 + Math.random() * 3000,
      );
    };
    schedule();
    return () => clearTimeout(timer);
  }, [idleBlink]);

  // Eyes follow the cursor.
  useEffect(() => {
    if (!follow) return;
    const onMove = (e: MouseEvent) => {
      const el = ref.current;
      if (!el) return;
      const r = el.getBoundingClientRect();
      const cx = r.left + r.width / 2;
      const cy = r.top + r.height * 0.32; // eyes sit high on the head
      // Smaller divisor = more sensitive; larger multiplier = more travel.
      // He really watches the cursor now.
      const dx = Math.max(-1, Math.min(1, (e.clientX - cx) / 180));
      const dy = Math.max(-1, Math.min(1, (e.clientY - cy) / 180));
      setIris({ x: dx * 8, y: dy * 7 });
    };
    window.addEventListener("mousemove", onMove);
    return () => window.removeEventListener("mousemove", onMove);
  }, [follow]);

  const handleClick = () => {
    if (!clickCroak) return;
    playRibbit(); // he says hello
    setCroak(true);
    setTimeout(() => setCroak(false), 900);
  };

  return (
    <span
      ref={ref}
      onClick={handleClick}
      style={{
        display: "inline-flex",
        cursor: clickCroak ? "pointer" : undefined,
      }}
    >
      <Mascot
        size={size}
        className={className}
        blink={blink}
        croak={croak}
        // Only hand over the sac while a live level is actually driving it. A
        // defined sacScale suppresses the croak class (FrogMascot), so passing a
        // resting 0 would silently kill the click-croak the rest of this
        // component exists to produce.
        sacScale={amp > 0 ? amp : undefined}
        irisDX={iris.x}
        irisDY={iris.y}
      />
    </span>
  );
};

export default LiveFrog;
