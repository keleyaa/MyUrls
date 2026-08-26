import { describe, expect, it } from 'vitest';

import { InvalidRequestError, UrlNotAllowedError } from '../errors.js';
import { normalizeTargetUrl } from './url-policy.js';

describe('target URL policy', () => {
  it('returns standard URL serialization for valid HTTP(S) targets', () => {
    expect(normalizeTargetUrl('HTTPS://Example.COM/docs?q=1#intro')).toBe(
      'https://example.com/docs?q=1#intro',
    );
  });

  it.each([
    'ftp://example.com/file',
    'javascript:alert(1)',
    'https://user:password@example.com/',
    'https://localhost/admin',
    'https://api.local/status',
    'https://service.internal/status',
    'https://router.home.arpa/',
    'https://127.0.0.1/',
    'https://10.0.0.8/',
    'https://169.254.1.2/',
    'https://192.168.1.3/',
    'https://0.0.0.0/',
    'https://224.0.0.1/',
    'https://192.0.2.10/',
    'https://[::1]/',
    'https://[fc00::1]/',
    'https://[fe80::1]/',
    'https://[2001:db8::1]/',
    'https://example.com/with space',
  ])('rejects unsafe target %s', (value) => {
    expect(() => normalizeTargetUrl(value)).toThrow(UrlNotAllowedError);
  });

  it('rejects control characters before URL parsing', () => {
    expect(() => normalizeTargetUrl('https://example.com/%0A')).not.toThrow();
    expect(() => normalizeTargetUrl('https://example.com/\u000a')).toThrow(UrlNotAllowedError);
  });

  it('enforces the UTF-8 byte limit as an invalid request', () => {
    const oversized = `https://example.com/${'é'.repeat(2040)}`;
    expect(() => normalizeTargetUrl(oversized)).toThrow(InvalidRequestError);
  });
});
