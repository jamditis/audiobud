/**
 * Whether any personalization data is stored, independent of the enable toggle.
 * Used to surface the view/export/reset controls even when the feature is off (#53).
 */
export function hasStoredPersonalizationData(
  p:
    | {
        learned_words?: string[] | null;
        dismissed_suggestions?: string[] | null;
      }
    | null
    | undefined,
): boolean {
  return (
    (p?.learned_words?.length ?? 0) > 0 ||
    (p?.dismissed_suggestions?.length ?? 0) > 0
  );
}
