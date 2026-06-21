// Plays a frog "ribbit" when the mascot is clicked. Primary source is the
// bundled recording (src/assets/ribbit.wav); if it can't play for any reason we
// fall back to a synthesized croak so the click always makes a sound.
import ribbitUrl from "../assets/ribbit.wav";

let audioEl: HTMLAudioElement | null = null;

export function playRibbit() {
  try {
    if (!audioEl) {
      audioEl = new Audio(ribbitUrl);
      audioEl.preload = "auto";
    }
    audioEl.currentTime = 0;
    const p = audioEl.play();
    if (p && typeof p.catch === "function") p.catch(() => synthRibbit());
  } catch {
    synthRibbit();
  }
}

// --- Synthesized fallback -------------------------------------------------
// Two pitch-bent sawtooth bursts (the "ri-bbit") through a low-pass filter with
// a ~33 Hz tremolo for the buzzy croak. Built only if the recording fails.
let ctx: AudioContext | null = null;

function burst(
  audio: AudioContext,
  start: number,
  dur: number,
  f0: number,
  f1: number,
) {
  const osc = audio.createOscillator();
  osc.type = "sawtooth";
  osc.frequency.setValueAtTime(f0, start);
  osc.frequency.linearRampToValueAtTime(f1, start + dur);

  const lfo = audio.createOscillator();
  lfo.type = "square";
  lfo.frequency.value = 33;
  const lfoGain = audio.createGain();
  lfoGain.gain.value = 0.4;

  const amp = audio.createGain();
  amp.gain.setValueAtTime(0.0001, start);
  amp.gain.exponentialRampToValueAtTime(0.4, start + 0.02);
  amp.gain.exponentialRampToValueAtTime(0.0001, start + dur);

  const lp = audio.createBiquadFilter();
  lp.type = "lowpass";
  lp.frequency.value = 900;

  lfo.connect(lfoGain).connect(amp.gain);
  osc.connect(lp).connect(amp).connect(audio.destination);

  osc.start(start);
  lfo.start(start);
  osc.stop(start + dur + 0.02);
  lfo.stop(start + dur + 0.02);
}

function synthRibbit() {
  try {
    const AudioCtor =
      window.AudioContext ||
      (window as unknown as { webkitAudioContext?: typeof AudioContext })
        .webkitAudioContext;
    if (!AudioCtor) return;
    ctx = ctx ?? new AudioCtor();
    if (ctx.state === "suspended") void ctx.resume();
    const now = ctx.currentTime;
    burst(ctx, now, 0.12, 210, 320); // "ri"
    burst(ctx, now + 0.16, 0.22, 180, 250); // "bbit"
  } catch {
    // Audio is a nice-to-have; never let it break the click.
  }
}
