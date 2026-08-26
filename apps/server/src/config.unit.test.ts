import { describe, expect, it } from 'vitest';

import { parseConfig } from './config.js';

const baseEnv: NodeJS.ProcessEnv = {
  NODE_ENV: 'test',
  PUBLIC_BASE_URL: 'https://myurl.example',
  IP_HASH_SECRET: 'test-secret-that-is-at-least-32-bytes-long',
  REDIS_URL: 'redis://:password@redis:6379/15',
  TURNSTILE_ENABLED: 'true',
  TURNSTILE_MODE: 'test',
  TURNSTILE_SITE_KEY: 'site-key',
  TURNSTILE_SECRET_KEY: 'secret-key',
  TURNSTILE_HOSTNAME: 'myurl.example',
};

describe('configuration', () => {
  it('parses valid test configuration and keeps proxy CIDRs structured', () => {
    const config = parseConfig({ ...baseEnv, TRUST_PROXY_CIDRS: '10.0.0.0/8, 2001:db8::/32' });
    expect(config.publicBaseOrigin).toBe('https://myurl.example');
    expect(config.redisUrl).toContain('password');
    expect(config.trustProxyCidrs).toHaveLength(2);
    expect(config.limits).toEqual({
      direct10m: 5,
      hard10m: 20,
      hard1d: 100,
      challengeScore: 3,
      blockScore: 8,
    });
  });

  it('merges a Compose Redis password into the client URL without requiring URL credentials', () => {
    const config = parseConfig({
      ...baseEnv,
      REDIS_URL: 'redis://redis:6379/15',
      REDIS_PASSWORD: 'p@ss/word',
    });
    expect(config.redisUrl).toContain(':p%40ss%2Fword@redis:6379/15');
    expect(() =>
      parseConfig({
        ...baseEnv,
        REDIS_PASSWORD: 'different-password',
      }),
    ).toThrow();
  });

  it.each([
    ['missing NODE_ENV', { ...baseEnv, NODE_ENV: undefined }],
    ['missing public base', { ...baseEnv, PUBLIC_BASE_URL: undefined }],
    ['short hash secret', { ...baseEnv, IP_HASH_SECRET: 'too-short' }],
    ['invalid public base path', { ...baseEnv, PUBLIC_BASE_URL: 'https://myurl.example/path' }],
    ['invalid Redis scheme', { ...baseEnv, REDIS_URL: 'http://redis:6379' }],
    ['invalid proxy CIDR', { ...baseEnv, TRUST_PROXY_CIDRS: 'not-a-cidr' }],
    ['out of range port', { ...baseEnv, APP_PORT: '65536' }],
    ['invalid limit relation', { ...baseEnv, CREATE_HARD_LIMIT_10M: '5' }],
    ['invalid boolean', { ...baseEnv, TURNSTILE_ENABLED: 'yes' }],
  ])('rejects %s', (_label, env) => {
    expect(() => parseConfig(env)).toThrow();
  });

  it('requires HTTPS and real Turnstile mode in production', () => {
    expect(() =>
      parseConfig({
        ...baseEnv,
        NODE_ENV: 'production',
        PUBLIC_BASE_URL: 'http://myurl.example',
        TURNSTILE_MODE: 'cloudflare',
      }),
    ).toThrow();
    expect(() =>
      parseConfig({
        ...baseEnv,
        NODE_ENV: 'production',
        TURNSTILE_MODE: 'test',
        PUBLIC_BASE_URL: 'https://myurl.example',
      }),
    ).toThrow();
  });

  it('rejects unbounded proxy trust in production', () => {
    const productionEnv = {
      ...baseEnv,
      NODE_ENV: 'production',
      PUBLIC_BASE_URL: 'https://myurl.example',
      TURNSTILE_MODE: 'cloudflare',
    };
    expect(() => parseConfig({ ...productionEnv, TRUST_PROXY_CIDRS: '0.0.0.0/0' })).toThrow();
    expect(() => parseConfig({ ...productionEnv, TRUST_PROXY_CIDRS: '::/0' })).toThrow();
  });

  it('allows disabled Turnstile and the memory store only in test/development', () => {
    const env = { ...baseEnv, TURNSTILE_ENABLED: 'false', TEST_STORE: 'memory' };
    const config = parseConfig(env);
    expect(config.turnstile.enabled).toBe(false);
    expect(config.testStore).toBe('memory');
    expect(() => parseConfig({ ...env, NODE_ENV: 'production' })).toThrow();
  });
});
