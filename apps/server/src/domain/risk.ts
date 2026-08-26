import type { AppConfig } from '../config.js';

export type RiskDecision = 'allow' | 'challenge' | 'block';

export interface RiskInput {
  tenMinuteCount: number;
  dailyCount: number;
  riskScore: number;
  limits: AppConfig['limits'];
  challengeEnabled: boolean;
  forceChallenge: boolean;
}

export function evaluateRisk(input: RiskInput): RiskDecision {
  if (
    input.tenMinuteCount > input.limits.hard10m ||
    input.dailyCount > input.limits.hard1d ||
    input.riskScore >= input.limits.blockScore
  ) {
    return 'block';
  }
  if (
    input.challengeEnabled &&
    (input.forceChallenge ||
      input.tenMinuteCount > input.limits.direct10m ||
      input.riskScore >= input.limits.challengeScore)
  ) {
    return 'challenge';
  }
  return 'allow';
}
