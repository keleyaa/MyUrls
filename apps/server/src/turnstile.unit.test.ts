import { afterEach, describe, expect, it, vi } from 'vitest';

import { CloudflareTurnstileVerifier } from './turnstile.js';
import { DependencyUnavailableError } from './errors.js';
import { makeTestConfig } from './testing/fake-store.js';

afterEach(() => {
  vi.unstubAllGlobals();
});

function response(payload: unknown, ok = true): Response {
  return { ok, json: async () => payload } as Response;
}

describe('Cloudflare Turnstile adapter', () => {
  it('accepts a successful production response with matching action and hostname', async () => {
    vi.stubGlobal(
      'fetch',
      vi
        .fn()
        .mockResolvedValue(
          response({ success: true, hostname: 'myurl.example', action: 'create_link' }),
        ),
    );
    const config = makeTestConfig({
      nodeEnv: 'production',
      turnstile: {
        enabled: true,
        mode: 'cloudflare',
        siteKey: 'site',
        secretKey: 'secret',
        hostname: 'myurl.example',
      },
    });
    await expect(new CloudflareTurnstileVerifier(config).verify('token')).resolves.toEqual({
      valid: true,
    });
  });

  it('rejects unsuccessful or mismatched validation without exposing the response', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(response({ success: false, 'error-codes': ['bad-request'] })),
    );
    const config = makeTestConfig();
    await expect(new CloudflareTurnstileVerifier(config).verify('token')).resolves.toEqual({
      valid: false,
    });

    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(response({ success: true, hostname: 'wrong', action: 'wrong' })),
    );
    const production = makeTestConfig({
      nodeEnv: 'production',
      turnstile: {
        enabled: true,
        mode: 'cloudflare',
        siteKey: 'site',
        secretKey: 'secret',
        hostname: 'myurl.example',
      },
    });
    await expect(new CloudflareTurnstileVerifier(production).verify('token')).resolves.toEqual({
      valid: false,
    });
  });

  it('maps bad transport or response data to dependency_unavailable', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(response({}, false)));
    await expect(
      new CloudflareTurnstileVerifier(makeTestConfig()).verify('token'),
    ).rejects.toBeInstanceOf(DependencyUnavailableError);
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(response('not-an-object')));
    await expect(
      new CloudflareTurnstileVerifier(makeTestConfig()).verify('token'),
    ).rejects.toBeInstanceOf(DependencyUnavailableError);
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('network')));
    await expect(
      new CloudflareTurnstileVerifier(makeTestConfig()).verify('token'),
    ).rejects.toBeInstanceOf(DependencyUnavailableError);
  });
});
