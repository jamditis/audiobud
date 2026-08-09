export const modelLoadingFailureMessage = (
  error: string | null | undefined,
  localizedFallback: string,
): string => (error?.trim() ? error : localizedFallback);
