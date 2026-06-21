import { useEffect, useRef, useState } from "react";
import FrogMascot from "./FrogMascot";
import { playRibbit } from "../../lib/ribbit";

// A FrogMascot that's alive: it blinks on its own, its eyes follow the cursor,
// and it croaks when clicked. Used for the wordmark and the sidebar nav icon.
interface LiveFrogProps {
  size?: number | string;
  className?: string;
  follow?: boolean;
  idleBlink?: boolean;
  clickCroak?: boolean;
}

const LiveFrog = ({
  size,
  className,
  follow = true,
  idleBlink = true,
  clickCroak = true,
}: LiveFrogProps) => {
  const ref = useRef<HTMLSpanElement>(null);
  const [blink, setBlink] = useState(false);
  const [croak, setCroak] = useState(false);
  const [iris, setIris] = useState({ x: 0, y: 0 });

  // Idle blink at a relaxed, slightly irregular cadence.
  useEffect(() => {
    if (!idleBlink) return;
    let timer: ReturnType<typeof setTimeout>;
    const schedule = () => {
      timer = setTimeout(() => {
        setBlink(true);
        setTimeout(() => setBlink(false), 130);
        schedule();
      }, 3000 + Math.random() * 3000);
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
      style={{ display: "inline-flex", cursor: clickCroak ? "pointer" : undefined }}
    >
      <FrogMascot
        size={size}
        className={className}
        blink={blink}
        croak={croak}
        irisDX={iris.x}
        irisDY={iris.y}
      />
    </span>
  );
};

export default LiveFrog;
