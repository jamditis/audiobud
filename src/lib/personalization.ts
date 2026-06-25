import type { PersonalizationData } from "@/bindings";

/**
 * Whether any personalization data is stored, independent of the enable toggle.
 * Used to surface the view/export/reset controls even when the feature is off (#53).
 *
 * Counts every stored-data field that `reset_personalization` clears -- learned words, learned
 * replacements, and dismissed suggestions -- so data saved in any of them is reachable while off.
 * Typed against the generated `PersonalizationData` so a backend field rename fails the build here.
 */
export function hasStoredPersonalizationData(
  p: PersonalizationData | null | undefined,
): boolean {
  return (
    (p?.learned_words?.length ?? 0) > 0 ||
    (p?.learned_replacements?.length ?? 0) > 0 ||
    (p?.dismissed_suggestions?.length ?? 0) > 0
  );
}
