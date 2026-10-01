import type { TFunction } from "i18next";

export const modelLoadingFailureMessage = (
  error: string | null | undefined,
  localizedFallback: string,
): string => (error?.trim() ? error : localizedFallback);

/** Load-failure code from `transcription.rs` for a non-UTF-8 Whisper path. */
export const WHISPER_MODEL_PATH_NOT_UTF8 = "whisper_model_path_not_utf8";

const MODEL_LOAD_ERROR_KEYS = new Map([
  [WHISPER_MODEL_PATH_NOT_UTF8, "errors.whisperModelPathNotUtf8"],
]);

/**
 * Replaces a backend load-failure code with its translated copy. Any other
 * error is returned unchanged, so existing backend detail still shows.
 */
export const localizeModelLoadError = (
  error: string | null | undefined,
  t: TFunction,
): string | null | undefined => {
  const key = error ? MODEL_LOAD_ERROR_KEYS.get(error) : undefined;
  return key ? t(key) : error;
};
