import pino from 'pino';
import { afterEach, describe, expect, it } from 'vitest';

import { buildApp } from './http.js';
import { ShortLinkService } from './service.js';
import { FakeStore, FakeTurnstile, makeTestConfig } from './testing/fake-store.js';

const apps: Array<ReturnType<typeof buildApp>> = [];

afterEach(async () => {
  while (apps.length > 0) {
    await apps.pop()?.close();
  }
});

function makeApp(
  configOverrides: Parameters<typeof makeTestConfig>[0] = {},
  store = new FakeStore(),
  turnstile = new FakeTurnstile(),
) {
  const config = makeTestConfig(configOverrides);
  let codeNumber = 0;
  const app = buildApp({
    config,
    store,
    turnstile,
    webRoot: '/path/that/does/not/exist',
    logger: pino({ enabled: false, base: null }),
    service: new ShortLinkService({
      config,
      store,
      turnstile,
      now: () => new Date('2026-08-26T04:00:00.000Z'),
      generateCode: () => `Code${String(++codeNumber).padStart(6, '0')}`,
    }),
  });
  apps.push(app);
  return { app, store, turnstile };
}

async function post(
  app: ReturnType<typeof buildApp>,
  payload: string,
  headers: Record<string, string> = {},
) {
  return app.inject({
    method: 'POST',
    url: '/api/v1/links',
    headers: { 'content-type': 'application/json', ...headers },
    payload,
  });
}

function errorCode(response: { json: () => unknown }): string {
  const body = response.json() as { error: { code: string } };
  return body.error.code;
}

describe('HTTP contract', () => {
  it('creates a link from the configured public base, regardless of Host', async () => {
    const { app } = makeApp();
    const response = await post(app, JSON.stringify({ url: 'https://example.com/docs' }), {
      host: 'attacker.example',
    });

    expect(response.statusCode).toBe(201);
    expect(response.json()).toMatchObject({
      code: 'Code000001',
      shortUrl: 'https://myurl.example/Code000001',
    });
  });

  it('rejects non-JSON, malformed, oversized, extra-property, and cross-origin requests', async () => {
    const { app } = makeApp();
    const wrongType = await app.inject({
      method: 'POST',
      url: '/api/v1/links',
      headers: { 'content-type': 'application/x-www-form-urlencoded' },
      payload: 'url=https%3A%2F%2Fexample.com',
    });
    expect(wrongType.statusCode).toBe(400);
    expect(errorCode(wrongType)).toBe('invalid_request');

    const malformed = await post(app, '{');
    expect(malformed.statusCode).toBe(400);
    expect(errorCode(malformed)).toBe('invalid_request');

    const extra = await post(app, JSON.stringify({ url: 'https://example.com', extra: true }));
    expect(extra.statusCode).toBe(400);
    expect(errorCode(extra)).toBe('invalid_request');

    const oversized = await post(
      app,
      JSON.stringify({ url: 'https://example.com/' + 'a'.repeat(17000) }),
    );
    expect(oversized.statusCode).toBe(400);
    expect(errorCode(oversized)).toBe('invalid_request');

    const crossOrigin = await post(app, JSON.stringify({ url: 'https://example.com' }), {
      origin: 'https://attacker.example',
    });
    expect(crossOrigin.statusCode).toBe(400);
    expect(errorCode(crossOrigin)).toBe('invalid_request');
  });

  it('maps URL and alias policy failures to stable JSON errors', async () => {
    const { app } = makeApp();
    const badUrl = await post(app, JSON.stringify({ url: 'http://localhost/admin' }));
    expect(badUrl.statusCode).toBe(422);
    expect(errorCode(badUrl)).toBe('url_not_allowed');

    const badAlias = await post(app, JSON.stringify({ url: 'https://example.com', alias: 'bad' }));
    expect(badAlias.statusCode).toBe(422);
    expect(errorCode(badAlias)).toBe('alias_invalid');
  });

  it('returns alias conflicts as 409 and does not expose submitted data', async () => {
    const { app, store } = makeApp();
    store.links.set('launch', 'https://existing.example/');
    const target = 'https://private.example/secret?token=not-for-logs';
    const token = 'secret-turnstile-token';
    const response = await post(
      app,
      JSON.stringify({ url: target, alias: 'launch', challengeToken: token }),
    );

    expect(response.statusCode).toBe(409);
    expect(errorCode(response)).toBe('alias_unavailable');
    expect(response.body).not.toContain(target);
    expect(response.body).not.toContain(token);
  });

  it('requires a challenge after the fifth direct request and accepts a valid token', async () => {
    const { app, turnstile } = makeApp({ limits: { direct10m: 1, hard10m: 20, hard1d: 100 } });
    const first = await post(app, JSON.stringify({ url: 'https://example.com/one' }));
    expect(first.statusCode).toBe(201);
    const required = await post(app, JSON.stringify({ url: 'https://example.com/two' }));
    expect(required.statusCode).toBe(403);
    expect(errorCode(required)).toBe('challenge_required');
    expect(required.json()).toMatchObject({
      challenge: { provider: 'turnstile', siteKey: 'site-key' },
    });

    const accepted = await post(
      app,
      JSON.stringify({ url: 'https://example.com/two', challengeToken: 'valid-token' }),
    );
    expect(accepted.statusCode).toBe(201);
    expect(turnstile.calls).toBe(1);
  });

  it('maps invalid and unavailable challenge responses', async () => {
    const invalidTurnstile = new FakeTurnstile();
    const { app: invalidApp } = makeApp(
      { limits: { direct10m: 1, hard10m: 20, hard1d: 100 } },
      new FakeStore(),
      invalidTurnstile,
    );
    await post(invalidApp, JSON.stringify({ url: 'https://example.com/one' }));
    const invalid = await post(
      invalidApp,
      JSON.stringify({ url: 'https://example.com/two', challengeToken: 'bad' }),
    );
    expect(invalid.statusCode).toBe(403);
    expect(errorCode(invalid)).toBe('challenge_invalid');

    const unavailable = new FakeTurnstile('valid-token', true);
    const { app: unavailableApp } = makeApp(
      { limits: { direct10m: 1, hard10m: 20, hard1d: 100 } },
      new FakeStore(),
      unavailable,
    );
    await post(unavailableApp, JSON.stringify({ url: 'https://example.com/one' }));
    const dependency = await post(
      unavailableApp,
      JSON.stringify({ url: 'https://example.com/two', challengeToken: 'valid-token' }),
    );
    expect(dependency.statusCode).toBe(503);
    expect(errorCode(dependency)).toBe('dependency_unavailable');
  });

  it('returns 429 at the hard limit', async () => {
    const { app } = makeApp({ limits: { direct10m: 1, hard10m: 2, hard1d: 3 } });
    expect((await post(app, JSON.stringify({ url: 'https://example.com/one' }))).statusCode).toBe(
      201,
    );
    expect((await post(app, JSON.stringify({ url: 'https://example.com/two' }))).statusCode).toBe(
      403,
    );
    const blocked = await post(app, JSON.stringify({ url: 'https://example.com/three' }));
    expect(blocked.statusCode).toBe(429);
    expect(errorCode(blocked)).toBe('rate_limited');
    expect(blocked.headers['retry-after']).toBe('120');
  });

  it('distinguishes live, ready, redirect, HEAD, missing, and Redis failure paths', async () => {
    const { app } = makeApp();
    expect((await app.inject({ method: 'GET', url: '/health/live' })).statusCode).toBe(200);
    expect((await app.inject({ method: 'GET', url: '/health/ready' })).statusCode).toBe(200);

    const degraded = makeApp({}, new FakeStore({ failPing: true }));
    expect((await degraded.app.inject({ method: 'GET', url: '/health/ready' })).statusCode).toBe(
      503,
    );

    const created = await post(app, JSON.stringify({ url: 'https://example.com/docs' }));
    const code = (created.json() as { code: string }).code;
    const redirect = await app.inject({ method: 'GET', url: `/${code}` });
    expect(redirect.statusCode).toBe(302);
    expect(redirect.headers.location).toBe('https://example.com/docs');
    expect(redirect.headers['cache-control']).toBe('no-store');
    expect(redirect.headers['referrer-policy']).toBe('no-referrer');
    expect(redirect.headers['x-robots-tag']).toBe('noindex, nofollow');

    const head = await app.inject({ method: 'HEAD', url: `/${code}` });
    expect(head.statusCode).toBe(302);
    expect(head.body).toBe('');

    const missing = await app.inject({ method: 'GET', url: '/missing-code' });
    expect(missing.statusCode).toBe(404);
    expect(missing.headers['content-type']).toContain('text/html');
    expect(missing.body).not.toContain('Redis');

    const failedStore = new FakeStore({ failLookup: true });
    const { app: failedApp } = makeApp({}, failedStore);
    const failed = await failedApp.inject({ method: 'GET', url: `/${code}` });
    expect(failed.statusCode).toBe(503);
    expect(failed.body).not.toContain('Redis');
  });

  it('rate-limits high-frequency short-link probes without exposing dependencies', async () => {
    const { app } = makeApp({ limits: { resolve10s: 1 } });
    const created = await post(app, JSON.stringify({ url: 'https://example.com/rate-limit' }));
    const code = (created.json() as { code: string }).code;

    expect((await app.inject({ method: 'GET', url: `/${code}` })).statusCode).toBe(302);
    const blocked = await app.inject({ method: 'GET', url: '/unknown-code' });
    expect(blocked.statusCode).toBe(429);
    expect(blocked.headers['retry-after']).toBe('10');
    expect(blocked.body).not.toContain('Redis');

    const blockedHead = await app.inject({ method: 'HEAD', url: '/another-code' });
    expect(blockedHead.statusCode).toBe(429);
    expect(blockedHead.body).toBe('');
  });

  it('returns a JSON 404 for unknown API routes', async () => {
    const { app } = makeApp();
    const response = await app.inject({ method: 'GET', url: '/api/v1/unknown' });
    expect(response.statusCode).toBe(404);
    expect(errorCode(response)).toBe('invalid_request');
  });
});
