export function transcriptionTimeoutSeconds(error: unknown): number | null {
  const message = error instanceof Error ? error.message : String(error);
  const match = /^Transcription timed out after (\d+)s$/.exec(message);

  return match ? Number(match[1]) : null;
}

export function parakeetInputTooLongSeconds(error: unknown): number | null {
  const message = error instanceof Error ? error.message : String(error);
  const match = /^parakeet_input_too_long:(\d+)$/.exec(message);

  return match ? Number(match[1]) : null;
}

export type TranscriptionErrorPresentation =
  | { kind: "parakeetInputTooLong"; seconds: number }
  | { kind: "generic" };

export function classifyTranscriptionError(
  error: unknown,
): TranscriptionErrorPresentation {
  const parakeetSeconds = parakeetInputTooLongSeconds(error);

  return parakeetSeconds === null
    ? { kind: "generic" }
    : { kind: "parakeetInputTooLong", seconds: parakeetSeconds };
}

export function recordingDurationLabel(totalSeconds: number): string {
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes}:${seconds.toString().padStart(2, "0")}`;
}
