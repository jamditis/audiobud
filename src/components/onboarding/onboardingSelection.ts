export interface OnboardingSelectionGuard {
  modelId: string | null;
}

export function claimOnboardingSelection(
  guard: OnboardingSelectionGuard,
  modelId: string,
): boolean {
  if (guard.modelId === modelId) {
    return false;
  }

  guard.modelId = modelId;
  return true;
}
