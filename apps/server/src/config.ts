import { URL } from 'node:url';

import type { Cidr } from './ip.js';
import { parseCidr } from './ip.js';

export const LINK_TTL_SECONDS = 7_776_000;
export const MAX_URL_BYTES = 4096;
export const MAX_BODY_BYTES = 16 * 1024;
export const AUTO_CODE_LENGTH = 8;
export const MAX_CODE_ATTEMPTS = 5;

export type NodeEnvironment = 'development' | 'test' | 'production';
export type TurnstileMode = 'cloudflare' | 'test';

export interface AppConfig {
  nodeEnv: NodeEnvironment;
  port: number;
  publicBaseUrl: string;
  publicBaseOrigin: string;
  redisUrl: string;
  ipHashSecret: Buffer;
  trustProxyCidrs: readonly Cidr[];
  turnstile: {
    enabled: boolean;
    mode: TurnstileMode;
    siteKey: string;
    secretKey: string;
    hostname: string;
  };
  limits: {
    direct10m: number;
    hard10m: number;
    hard1d: number;
    challengeScore: number;
    blockScore: number;
  };
  redisTimeoutMs: number;
  turnstileTimeoutMs: number;
  requestTimeoutMs: number;
  shutdownTimeoutMs: number;
  testForceChallenge: boolean;
  testStore: 'memory' | undefined;
}

function required(env: NodeJS.ProcessEnv, name: string): string {
  const value = env[name];
  if (value === undefined || value.length === 0) {
    throw new Error(`Missing required configuration: ${name}`);
  }
  return value;
}

function parseInteger(
  env: NodeJS.ProcessEnv,
  name: string,
  fallback: number,
  minimum: number,
  maximum = Number.MAX_SAFE_INTEGER,
): number {
  const raw = env[name] ?? String(fallback);
  if (!/^\d+$/.test(raw)) {
    throw new Error(`Invalid numeric configuration: ${name}`);
  }
  const value = Number(raw);
  if (!Number.isSafeInteger(value) || value < minimum || value > maximum) {
    throw new Error(`Invalid numeric configuration: ${name}`);
  }
  return value;
}

function parseBoolean(env: NodeJS.ProcessEnv, name: string, fallback: boolean): boolean {
  const raw = env[name] ?? String(fallback);
  if (raw !== 'true' && raw !== 'false') {
    throw new Error(`Invalid boolean configuration: ${name}`);
  }
  return raw === 'true';
}

function parsePublicBaseUrl(
  raw: string,
  nodeEnv: NodeEnvironment,
): { url: string; origin: string } {
  let parsed: URL;
  try {
    parsed = new URL(raw);
  } catch {
    throw new Error('Invalid PUBLIC_BASE_URL');
  }
  if (
    !['http:', 'https:'].includes(parsed.protocol) ||
    parsed.username !== '' ||
    parsed.password !== '' ||
    parsed.pathname !== '/' ||
    parsed.search !== '' ||
    parsed.hash !== ''
  ) {
    throw new Error('Invalid PUBLIC_BASE_URL');
  }
  if (nodeEnv === 'production' && parsed.protocol !== 'https:') {
    throw new Error('PUBLIC_BASE_URL must use HTTPS in production');
  }
  return { url: parsed.origin, origin: parsed.origin };
}

function validateRedisUrl(raw: string, password: string): string {
  let parsed: URL;
  try {
    parsed = new URL(raw);
  } catch {
    throw new Error('Invalid REDIS_URL');
  }
  if (!['redis:', 'rediss:'].includes(parsed.protocol) || parsed.hostname === '') {
    throw new Error('Invalid REDIS_URL');
  }
  if (parsed.search !== '' || parsed.hash !== '') {
    throw new Error('Invalid REDIS_URL');
  }
  const database =
    parsed.pathname === '' || parsed.pathname === '/' ? 0 : Number(parsed.pathname.slice(1));
  if (!Number.isInteger(database) || database < 0 || database > 15) {
    throw new Error('Invalid REDIS_URL');
  }
  if (password !== '') {
    try {
      if (parsed.password !== '' && decodeURIComponent(parsed.password) !== password) {
        throw new Error('Redis password mismatch');
      }
      parsed.password = password;
    } catch {
      throw new Error('Invalid REDIS_PASSWORD');
    }
  }
  return parsed.toString();
}

function parseTrustProxyCidrs(
  raw: string | undefined,
  nodeEnv: NodeEnvironment,
): readonly Cidr[] {
  if (raw === undefined || raw.trim() === '') {
    return [];
  }
  try {
    return raw
      .split(',')
      .map((part) => part.trim())
      .filter((part) => part !== '')
      .map((part) => {
        const cidr = parseCidr(part);
        if (nodeEnv === 'production' && cidr[1] === 0) {
          throw new Error('Unbounded proxy trust is not allowed in production');
        }
        return cidr;
      });
  } catch {
    throw new Error('Invalid TRUST_PROXY_CIDRS');
  }
}

export function parseConfig(env: NodeJS.ProcessEnv): AppConfig {
  const nodeEnvRaw = required(env, 'NODE_ENV');
  if (!['development', 'test', 'production'].includes(nodeEnvRaw)) {
    throw new Error('Invalid NODE_ENV');
  }
  const nodeEnv = nodeEnvRaw as NodeEnvironment;
  const publicBase = parsePublicBaseUrl(required(env, 'PUBLIC_BASE_URL'), nodeEnv);
  const secretRaw = required(env, 'IP_HASH_SECRET');
  if (Buffer.byteLength(secretRaw, 'utf8') < 32) {
    throw new Error('IP_HASH_SECRET must contain at least 32 bytes');
  }
  if (nodeEnv === 'production' && secretRaw.includes('replace-with')) {
    throw new Error('IP_HASH_SECRET must not use an example value');
  }

  const direct10m = parseInteger(env, 'CREATE_DIRECT_LIMIT_10M', 5, 1);
  const hard10m = parseInteger(env, 'CREATE_HARD_LIMIT_10M', 20, 1);
  const hard1d = parseInteger(env, 'CREATE_HARD_LIMIT_1D', 100, 1);
  const challengeScore = parseInteger(env, 'RISK_CHALLENGE_SCORE', 3, 0);
  const blockScore = parseInteger(env, 'RISK_BLOCK_SCORE', 8, 0);
  if (hard10m <= direct10m || hard1d <= hard10m || blockScore <= challengeScore) {
    throw new Error('Invalid limit relationship');
  }

  const turnstileEnabled = parseBoolean(env, 'TURNSTILE_ENABLED', true);
  const turnstileModeRaw = env.TURNSTILE_MODE ?? 'cloudflare';
  if (turnstileModeRaw !== 'cloudflare' && turnstileModeRaw !== 'test') {
    throw new Error('Invalid TURNSTILE_MODE');
  }
  const turnstileMode = turnstileModeRaw as TurnstileMode;
  const siteKey = env.TURNSTILE_SITE_KEY ?? '';
  const secretKey = env.TURNSTILE_SECRET_KEY ?? '';
  const hostname = env.TURNSTILE_HOSTNAME ?? '';
  if (turnstileEnabled && (siteKey === '' || secretKey === '')) {
    throw new Error('Turnstile keys are required when enabled');
  }
  if (
    nodeEnv === 'production' &&
    (!turnstileEnabled || turnstileMode !== 'cloudflare' || hostname === '')
  ) {
    throw new Error('Production Turnstile configuration is incomplete');
  }
  if (turnstileMode === 'test' && nodeEnv !== 'test') {
    throw new Error('Test Turnstile mode is only available in test environment');
  }

  const testForceChallenge = parseBoolean(env, 'TEST_FORCE_CHALLENGE', false);
  if (testForceChallenge && nodeEnv !== 'test') {
    throw new Error('TEST_FORCE_CHALLENGE is only available in test environment');
  }
  const testStoreRaw = env.TEST_STORE;
  if (testStoreRaw !== undefined && (testStoreRaw !== 'memory' || nodeEnv !== 'test')) {
    throw new Error('TEST_STORE is only available as memory in test environment');
  }

  return {
    nodeEnv,
    port: parseInteger(env, 'APP_PORT', 3000, 1, 65535),
    publicBaseUrl: publicBase.url,
    publicBaseOrigin: publicBase.origin,
    redisUrl: validateRedisUrl(env.REDIS_URL ?? 'redis://redis:6379/0', env.REDIS_PASSWORD ?? ''),
    ipHashSecret: Buffer.from(secretRaw, 'utf8'),
    trustProxyCidrs: parseTrustProxyCidrs(env.TRUST_PROXY_CIDRS, nodeEnv),
    turnstile: {
      enabled: turnstileEnabled,
      mode: turnstileMode,
      siteKey,
      secretKey,
      hostname,
    },
    limits: { direct10m, hard10m, hard1d, challengeScore, blockScore },
    redisTimeoutMs: parseInteger(env, 'REDIS_TIMEOUT_MS', 750, 1),
    turnstileTimeoutMs: parseInteger(env, 'TURNSTILE_TIMEOUT_MS', 2500, 1),
    requestTimeoutMs: parseInteger(env, 'REQUEST_TIMEOUT_MS', 10000, 1),
    shutdownTimeoutMs: parseInteger(env, 'SHUTDOWN_TIMEOUT_MS', 10000, 1),
    testForceChallenge,
    testStore: testStoreRaw === 'memory' ? 'memory' : undefined,
  };
}
