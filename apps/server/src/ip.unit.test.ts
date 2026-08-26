import { describe, expect, it } from 'vitest';

import { fingerprintIp, getClientIp, parseCidr } from './ip.js';

describe('client IP handling', () => {
  it('uses the direct peer when no proxy is trusted', () => {
    expect(getClientIp('127.0.0.1', { 'x-forwarded-for': '198.51.100.4' }, [])).toBe('127.0.0.1');
  });

  it('uses forwarded client data only through trusted proxies', () => {
    const trusted = [parseCidr('10.0.0.0/8')];
    expect(getClientIp('10.0.0.8', { 'x-forwarded-for': '198.51.100.4' }, trusted)).toBe(
      '198.51.100.4',
    );
    expect(getClientIp('192.168.1.8', { 'x-forwarded-for': '198.51.100.4' }, trusted)).toBe(
      '192.168.1.8',
    );
  });

  it('supports the Forwarded header and canonicalizes mapped IPv4', () => {
    const trusted = [parseCidr('127.0.0.0/8')];
    expect(getClientIp('::ffff:127.0.0.1', { forwarded: 'for="198.51.100.7"' }, trusted)).toBe(
      '198.51.100.7',
    );
  });

  it('creates a fixed-length HMAC fingerprint without returning the address', () => {
    const fingerprint = fingerprintIp(Buffer.from('secret'), '203.0.113.7');
    expect(fingerprint).toMatch(/^[0-9a-f]{64}$/);
    expect(fingerprint).not.toContain('203.0.113.7');
  });
});
