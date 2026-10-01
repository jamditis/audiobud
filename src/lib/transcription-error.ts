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

/**
 * Backend code when a transcription gave up waiting for a model load that is
 * still running (issue #90). Not a stuck engine: a retry can succeed.
 */
export const MODEL_STILL_LOADING = "model_still_loading";

export function isModelStillLoading(error: unknown): boolean {
  const message = error instanceof Error ? error.message : String(error);
  return message === MODEL_STILL_LOADING;
}

export type TranscriptionErrorPresentation =
  | { kind: "parakeetInputTooLong"; seconds: number }
  | { kind: "modelStillLoading" }
  | { kind: "generic" };

export function classifyTranscriptionError(
  error: unknown,
): TranscriptionErrorPresentation {
  if (isModelStillLoading(error)) {
    return { kind: "modelStillLoading" };
  }
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
