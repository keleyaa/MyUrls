import { createClient, type RedisClientType } from 'redis';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';

import { LINK_TTL_SECONDS } from './config.js';
import { DependencyUnavailableError } from './errors.js';
import { RedisLinkStore } from './redis.js';

const redisUrl = process.env.REDIS_URL;
let store: RedisLinkStore;
let client: RedisClientType;

beforeAll(async () => {
  if (redisUrl === undefined) {
    throw new Error('REDIS_URL is required for real Redis integration tests');
  }
  client = createClient({ url: redisUrl });
  client.on('error', () => undefined);
  await client.connect();
  await client.flushDb();
  store = await RedisLinkStore.connect(redisUrl, 1500);
});

afterAll(async () => {
  await store?.close();
  if (client?.isOpen) {
    await client.quit();
  }
});

describe('RedisLinkStore with a real Redis server', () => {
  it('claims with NX and keeps the 90-day TTL', async () => {
    expect(await store.claim('launch', 'https://example.com/launch', LINK_TTL_SECONDS)).toBe(true);
    expect(await store.claim('launch', 'https://example.com/replaced', LINK_TTL_SECONDS)).toBe(
      false,
    );
    expect(await store.lookup('launch')).toBe('https://example.com/launch');
    const ttl = await client.ttl('myurl:link:launch');
    expect(ttl).toBeGreaterThan(LINK_TTL_SECONDS - 5);
  });

  it('increments rate counters atomically and sets TTL on first write', async () => {
    const fingerprint = 'a'.repeat(64);
    expect(await store.incrementCreateCounters(fingerprint, '2026-08-26')).toEqual({
      tenMinuteCount: 1,
      dailyCount: 1,
    });
    expect(await store.incrementCreateCounters(fingerprint, '2026-08-26')).toEqual({
      tenMinuteCount: 2,
      dailyCount: 2,
    });
    expect(await client.ttl(`myurl:rate:create:10m:${fingerprint}`)).toBeGreaterThan(590);
    expect(await client.ttl(`myurl:rate:create:1d:2026-08-26:${fingerprint}`)).toBeGreaterThan(
      172790,
    );
  });

  it('increments risk atomically with a bounded TTL', async () => {
    const fingerprint = 'b'.repeat(64);
    expect(await store.addRiskScore(fingerprint, 3)).toBe(3);
    expect(await store.addRiskScore(fingerprint, 1)).toBe(4);
    expect(await store.getRiskScore(fingerprint)).toBe(4);
    expect(await client.ttl(`myurl:risk:create:10m:${fingerprint}`)).toBeGreaterThan(590);
  });

  it('allows exactly one concurrent winner for the same alias', async () => {
    const results = await Promise.all(
      Array.from({ length: 20 }, (_, index) =>
        store.claim('concurrent', `https://example.com/${index}`, LINK_TTL_SECONDS),
      ),
    );
    expect(results.filter(Boolean)).toHaveLength(1);
  });

  it('does not overwrite an existing code and observes expiry', async () => {
    expect(await store.claim('collision', 'https://old.example/', LINK_TTL_SECONDS)).toBe(true);
    expect(await store.claim('collision', 'https://new.example/', LINK_TTL_SECONDS)).toBe(false);
    expect(await store.lookup('collision')).toBe('https://old.example/');
    await client.set('myurl:link:expires', 'https://example.com/expired', { EX: 1 });
    await new Promise((resolve) => setTimeout(resolve, 1100));
    expect(await store.lookup('expires')).toBeUndefined();
  });

  it('normalizes a closed-client failure as dependency_unavailable', async () => {
    await store.close();
    await expect(store.lookup('launch')).rejects.toBeInstanceOf(DependencyUnavailableError);
  });
});
