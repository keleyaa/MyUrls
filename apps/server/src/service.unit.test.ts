import { describe, expect, it } from 'vitest';

import { LINK_TTL_SECONDS } from './config.js';
import {
  AliasUnavailableError,
  ChallengeInvalidError,
  ChallengeRequiredError,
  CodeGenerationExhaustedError,
  DependencyUnavailableError,
  MyUrlError,
  RateLimitedError,
} from './errors.js';
import { fingerprintIp } from './ip.js';
import { ShortLinkService, createDefaultService } from './service.js';
import { FakeStore, FakeTurnstile, makeTestConfig } from './testing/fake-store.js';

const fixedNow = () => new Date('2026-08-26T04:00:00.000Z');

function makeService(
  store = new FakeStore(),
  turnstile = new FakeTurnstile(),
  config = makeTestConfig(),
  generateCode?: () => string,
): ShortLinkService {
  const options = { config, store, turnstile, now: fixedNow };
  return generateCode === undefined
    ? new ShortLinkService(options)
    : new ShortLinkService({ ...options, generateCode });
}

describe('ShortLinkService', () => {
  it('creates an automatic short link with a fixed expiry', async () => {
    const store = new FakeStore();
    const service = makeService(store, new FakeTurnstile(), makeTestConfig(), () => 'Abcd1234');

    const result = await service.create(
      { url: 'HTTPS://Example.COM/docs?q=1' },
      { clientIp: '198.51.100.4' },
    );

    expect(result).toEqual({
      code: 'Abcd1234',
      shortUrl: 'https://myurl.example/Abcd1234',
      expiresAt: new Date(fixedNow().getTime() + LINK_TTL_SECONDS * 1000).toISOString(),
    });
    expect(store.links.get('Abcd1234')).toBe('https://example.com/docs?q=1');
  });

  it('uses secure defaults when optional service dependencies are omitted', async () => {
    const store = new FakeStore();
    const service = createDefaultService({
      config: makeTestConfig(),
      store,
      turnstile: new FakeTurnstile(),
    });
    const result = await service.create(
      { url: 'https://example.com/defaults' },
      { clientIp: '198.51.100.4' },
    );
    expect(result.code).toMatch(/^[0-9A-Za-z]{8}$/);
  });

  it('normalizes and stores a custom alias', async () => {
    const store = new FakeStore();
    const service = makeService(store);

    const result = await service.create(
      { url: 'https://example.com', alias: '  Launch_42 ' },
      { clientIp: '198.51.100.4' },
    );

    expect(result.code).toBe('launch_42');
    expect(store.links.get('launch_42')).toBe('https://example.com/');
  });

  it('adds risk for a rejected URL and preserves its domain error', async () => {
    const store = new FakeStore();
    const service = makeService(store);
    const fingerprint = fingerprintIp(makeTestConfig().ipHashSecret, '198.51.100.4');

    await expect(
      service.create({ url: 'http://localhost/admin' }, { clientIp: '198.51.100.4' }),
    ).rejects.toBeInstanceOf(MyUrlError);
    expect(store.risks.get(fingerprint)).toBe(1);
  });

  it('adds risk for invalid aliases and reserved aliases', async () => {
    const store = new FakeStore();
    const service = makeService(store);
    const fingerprint = fingerprintIp(makeTestConfig().ipHashSecret, '198.51.100.4');

    await expect(
      service.create({ url: 'https://example.com', alias: 'bad' }, { clientIp: '198.51.100.4' }),
    ).rejects.toMatchObject({ code: 'alias_invalid' });
    await expect(
      service.create({ url: 'https://example.com', alias: 'HEALTH' }, { clientIp: '198.51.100.4' }),
    ).rejects.toBeInstanceOf(AliasUnavailableError);
    expect(store.risks.get(fingerprint)).toBe(2);
  });

  it('returns alias conflict and records the risk point', async () => {
    const store = new FakeStore();
    store.links.set('launch', 'https://already.example/');
    const service = makeService(store);
    const fingerprint = fingerprintIp(makeTestConfig().ipHashSecret, '198.51.100.4');

    await expect(
      service.create({ url: 'https://example.com', alias: 'launch' }, { clientIp: '198.51.100.4' }),
    ).rejects.toBeInstanceOf(AliasUnavailableError);
    expect(store.risks.get(fingerprint)).toBe(1);
  });

  it('requires and accepts a valid challenge after the direct threshold', async () => {
    const store = new FakeStore();
    const turnstile = new FakeTurnstile();
    const config = makeTestConfig({ limits: { direct10m: 1, hard10m: 20, hard1d: 100 } });
    const service = makeService(
      store,
      turnstile,
      config,
      (() => {
        let count = 0;
        return () => `Code${++count}123`;
      })(),
    );

    await service.create({ url: 'https://example.com/first' }, { clientIp: '198.51.100.4' });
    await expect(
      service.create({ url: 'https://example.com/second' }, { clientIp: '198.51.100.4' }),
    ).rejects.toBeInstanceOf(ChallengeRequiredError);
    const result = await service.create(
      { url: 'https://example.com/third', challengeToken: 'valid-token' },
      { clientIp: '198.51.100.4' },
    );
    expect(result.code).toBe('Code2123');
    expect(turnstile.calls).toBe(1);
  });

  it('returns challenge_invalid and adds three risk points for a bad token', async () => {
    const store = new FakeStore();
    const turnstile = new FakeTurnstile();
    const config = makeTestConfig({ limits: { direct10m: 1, hard10m: 20, hard1d: 100 } });
    const service = makeService(store, turnstile, config, () => 'Code1234');
    const fingerprint = fingerprintIp(config.ipHashSecret, '198.51.100.4');

    await service.create({ url: 'https://example.com/first' }, { clientIp: '198.51.100.4' });
    await expect(
      service.create(
        { url: 'https://example.com/second', challengeToken: 'bad-token' },
        { clientIp: '198.51.100.4' },
      ),
    ).rejects.toBeInstanceOf(ChallengeInvalidError);
    expect(store.risks.get(fingerprint)).toBe(3);
  });

  it('maps an unavailable challenge dependency', async () => {
    const store = new FakeStore();
    const turnstile = new FakeTurnstile('valid-token', true);
    const config = makeTestConfig({ limits: { direct10m: 1, hard10m: 20, hard1d: 100 } });
    const service = makeService(store, turnstile, config, () => 'Code1234');

    await service.create({ url: 'https://example.com/first' }, { clientIp: '198.51.100.4' });
    await expect(
      service.create(
        { url: 'https://example.com/second', challengeToken: 'valid-token' },
        { clientIp: '198.51.100.4' },
      ),
    ).rejects.toBeInstanceOf(DependencyUnavailableError);
  });

  it('blocks hard limits and existing risk scores', async () => {
    const store = new FakeStore();
    const config = makeTestConfig({ limits: { direct10m: 1, hard10m: 2, hard1d: 3 } });
    const service = makeService(store, new FakeTurnstile(), config, () => 'Code1234');

    await service.create({ url: 'https://example.com/first' }, { clientIp: '198.51.100.4' });
    await expect(
      service.create({ url: 'https://example.com/second' }, { clientIp: '198.51.100.4' }),
    ).rejects.toBeInstanceOf(ChallengeRequiredError);
    await expect(
      service.create({ url: 'https://example.com/third' }, { clientIp: '198.51.100.4' }),
    ).rejects.toBeInstanceOf(RateLimitedError);

    const riskStore = new FakeStore();
    const riskFingerprint = fingerprintIp(config.ipHashSecret, '203.0.113.10');
    riskStore.risks.set(riskFingerprint, 8);
    await expect(
      makeService(riskStore, new FakeTurnstile(), config, () => 'Code5678').create(
        { url: 'https://example.com' },
        { clientIp: '203.0.113.10' },
      ),
    ).rejects.toBeInstanceOf(RateLimitedError);
  });

  it('retries generated collisions and reports exhaustion without overwriting', async () => {
    const store = new FakeStore();
    store.links.set('taken123', 'https://old.example/');
    let calls = 0;
    const service = makeService(store, new FakeTurnstile(), makeTestConfig(), () => {
      calls += 1;
      return calls < 3 ? 'taken123' : 'fresh123';
    });
    const result = await service.create(
      { url: 'https://example.com' },
      { clientIp: '198.51.100.4' },
    );
    expect(result.code).toBe('fresh123');
    expect(store.links.get('taken123')).toBe('https://old.example/');

    const exhaustedStore = new FakeStore();
    exhaustedStore.links.set('taken123', 'https://old.example/');
    await expect(
      makeService(exhaustedStore, new FakeTurnstile(), makeTestConfig(), () => 'taken123').create(
        { url: 'https://example.com' },
        { clientIp: '198.51.100.4' },
      ),
    ).rejects.toBeInstanceOf(CodeGenerationExhaustedError);
  });

  it('skips reserved or malformed generator output and resolves only valid codes', async () => {
    const store = new FakeStore();
    store.links.set('valid1', 'https://example.com/');
    let call = 0;
    const service = makeService(store, new FakeTurnstile(), makeTestConfig(), () => {
      call += 1;
      return call === 1 ? 'api' : 'valid1';
    });
    await expect(
      service.create({ url: 'https://example.com' }, { clientIp: '198.51.100.4' }),
    ).rejects.toBeInstanceOf(CodeGenerationExhaustedError);
    await expect(service.resolve('bad')).resolves.toBeUndefined();
    await expect(service.resolve('valid1')).resolves.toBe('https://example.com/');
    expect(
      createDefaultService({ config: makeTestConfig(), store, turnstile: new FakeTurnstile() }),
    ).toBeInstanceOf(ShortLinkService);
  });

  it('returns a dependency error when risk recording fails', async () => {
    const config = makeTestConfig();
    const service = makeService(new FakeStore({ failAddRisk: true }), new FakeTurnstile(), config);
    await expect(
      service.create({ url: 'https://localhost' }, { clientIp: '198.51.100.4' }),
    ).rejects.toBeInstanceOf(DependencyUnavailableError);
  });
});
