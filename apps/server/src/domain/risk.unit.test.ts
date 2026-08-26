import { describe, expect, it } from 'vitest';

import { evaluateRisk } from './risk.js';

const limits = {
  direct10m: 5,
  hard10m: 20,
  hard1d: 100,
  challengeScore: 3,
  blockScore: 8,
};

describe('risk decisions', () => {
  it('allows low-risk traffic through', () => {
    expect(
      evaluateRisk({
        tenMinuteCount: 1,
        dailyCount: 1,
        riskScore: 0,
        limits,
        challengeEnabled: true,
        forceChallenge: false,
      }),
    ).toBe('allow');
  });

  it('requires challenge after direct threshold or risk score', () => {
    expect(
      evaluateRisk({
        tenMinuteCount: 6,
        dailyCount: 6,
        riskScore: 0,
        limits,
        challengeEnabled: true,
        forceChallenge: false,
      }),
    ).toBe('challenge');
    expect(
      evaluateRisk({
        tenMinuteCount: 1,
        dailyCount: 1,
        riskScore: 3,
        limits,
        challengeEnabled: true,
        forceChallenge: false,
      }),
    ).toBe('challenge');
  });

  it('blocks hard limits and risk score', () => {
    expect(
      evaluateRisk({
        tenMinuteCount: 21,
        dailyCount: 21,
        riskScore: 0,
        limits,
        challengeEnabled: true,
        forceChallenge: false,
      }),
    ).toBe('block');
    expect(
      evaluateRisk({
        tenMinuteCount: 1,
        dailyCount: 101,
        riskScore: 0,
        limits,
        challengeEnabled: true,
        forceChallenge: false,
      }),
    ).toBe('block');
    expect(
      evaluateRisk({
        tenMinuteCount: 1,
        dailyCount: 1,
        riskScore: 8,
        limits,
        challengeEnabled: true,
        forceChallenge: false,
      }),
    ).toBe('block');
  });

  it('bypasses challenge only when the provider is disabled', () => {
    expect(
      evaluateRisk({
        tenMinuteCount: 6,
        dailyCount: 6,
        riskScore: 0,
        limits,
        challengeEnabled: false,
        forceChallenge: true,
      }),
    ).toBe('allow');
    expect(
      evaluateRisk({
        tenMinuteCount: 1,
        dailyCount: 1,
        riskScore: 0,
        limits,
        challengeEnabled: true,
        forceChallenge: true,
      }),
    ).toBe('challenge');
  });
});
