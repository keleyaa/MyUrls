import type { AppConfig } from '../config.js';
import type { LinkStore } from '../ports.js';

export interface FakeStoreOptions {
  failCounters?: boolean;
  failLookup?: boolean;
  failPing?: boolean;
  failRisk?: boolean;
  failAddRisk?: boolean;
}

export class FakeStore implements LinkStore {
  readonly links = new Map<string, string>();
  readonly risks = new Map<string, number>();
  private readonly counts = new Map<string, number>();
  private readonly options: FakeStoreOptions;

  constructor(options: FakeStoreOptions = {}) {
    this.options = options;
  }

  async claim(code: string, targetUrl: string, _ttlSeconds: number): Promise<boolean> {
    if (this.links.has(code)) {
      return false;
    }
    this.links.set(code, targetUrl);
    return true;
  }

  async lookup(code: string): Promise<string | undefined> {
    if (this.options.failLookup) {
      throw new Error('lookup failed');
    }
    return this.links.get(code);
  }

  async incrementResolveCounter(fingerprint: string): Promise<number> {
    if (this.options.failCounters) {
      throw new Error('counter failed');
    }
    const key = `resolve:${fingerprint}`;
    const next = (this.counts.get(key) ?? 0) + 1;
    this.counts.set(key, next);
    return next;
  }

  async incrementCreateCounters(
    fingerprint: string,
    _utcDate: string,
  ): Promise<{ tenMinuteCount: number; dailyCount: number }> {
    if (this.options.failCounters) {
      throw new Error('counter failed');
    }
    const next = (this.counts.get(fingerprint) ?? 0) + 1;
    this.counts.set(fingerprint, next);
    return { tenMinuteCount: next, dailyCount: next };
  }

  async getRiskScore(fingerprint: string): Promise<number> {
    if (this.options.failRisk) {
      throw new Error('risk failed');
    }
    return this.risks.get(fingerprint) ?? 0;
  }

  async addRiskScore(fingerprint: string, points: number): Promise<number> {
    if (this.options.failRisk || this.options.failAddRisk) {
      throw new Error('risk failed');
    }
    const next = (this.risks.get(fingerprint) ?? 0) + points;
    this.risks.set(fingerprint, next);
    return next;
  }

  async ping(): Promise<void> {
    if (this.options.failPing) {
      throw new Error('ping failed');
    }
  }

  async close(): Promise<void> {}
}

export class FakeTurnstile {
  calls = 0;
  constructor(
    private readonly validToken = 'valid-token',
    private readonly unavailable = false,
  ) {}

  async verify(token: string): Promise<{ valid: boolean }> {
    this.calls += 1;
    if (this.unavailable) {
      throw new Error('turnstile unavailable');
    }
    return { valid: token === this.validToken };
  }
}

export type TestConfigOverrides = Partial<Omit<AppConfig, 'turnstile' | 'limits'>> & {
  turnstile?: Partial<AppConfig['turnstile']>;
  limits?: Partial<AppConfig['limits']>;
};

export function makeTestConfig(overrides: TestConfigOverrides = {}): AppConfig {
  const base: AppConfig = {
    nodeEnv: 'test',
    port: 3000,
    publicBaseUrl: 'https://myurl.example',
    publicBaseOrigin: 'https://myurl.example',
    redisUrl: 'redis://127.0.0.1:6379/15',
    ipHashSecret: Buffer.from('test-secret-that-is-at-least-32-bytes-long'),
    trustProxyCidrs: [],
    turnstile: {
      enabled: true,
      mode: 'test',
      siteKey: 'site-key',
      secretKey: 'secret-key',
      hostname: 'myurl.example',
    },
    limits: {
      direct10m: 5,
      hard10m: 20,
      hard1d: 100,
      resolve10s: 600,
      challengeScore: 3,
      blockScore: 8,
    },
    redisTimeoutMs: 100,
    turnstileTimeoutMs: 100,
    requestTimeoutMs: 1000,
    shutdownTimeoutMs: 1000,
    testForceChallenge: false,
    testStore: undefined,
  };
  return {
    ...base,
    ...overrides,
    turnstile: { ...base.turnstile, ...(overrides.turnstile ?? {}) },
    limits: { ...base.limits, ...(overrides.limits ?? {}) },
  };
}
