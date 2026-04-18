/**
 * Beta Features Type System
 *
 * No beta features currently active.
 * When adding a new beta feature:
 * 1. Add property to BetaFeatures interface
 * 2. Add default in DEFAULT_BETA_FEATURES
 * 3. Add strings in BETA_FEATURE_NAMES and BETA_FEATURE_DESCRIPTIONS
 */

export interface BetaFeatures {}

export const DEFAULT_BETA_FEATURES: BetaFeatures = {};

export const BETA_FEATURE_NAMES: Record<keyof BetaFeatures, string> = {};

export const BETA_FEATURE_DESCRIPTIONS: Record<keyof BetaFeatures, string> = {};

export type BetaFeatureKey = keyof BetaFeatures;

export function loadBetaFeatures(): BetaFeatures {
  return {};
}

export function saveBetaFeatures(_features: BetaFeatures): void {}
