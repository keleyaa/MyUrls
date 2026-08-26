import { Buffer } from 'node:buffer';

import { InvalidRequestError, UrlNotAllowedError } from '../errors.js';
import { parseIpAddress } from '../ip.js';

const BLOCKED_IP_RANGES = new Set([
  'unspecified',
  'broadcast',
  'multicast',
  'linkLocal',
  'loopback',
  'private',
  'uniqueLocal',
  'carrierGradeNat',
  'benchmark',
  'reserved',
  'documentation',
]);

const BLOCKED_HOST_SUFFIXES = ['localhost', 'local', 'internal', 'home.arpa'];

function isBlockedHostname(hostname: string): boolean {
  const normalized = hostname.toLowerCase().replace(/\.+$/, '');
  return BLOCKED_HOST_SUFFIXES.some(
    (suffix) => normalized === suffix || normalized.endsWith(`.${suffix}`),
  );
}

function isBlockedIpLiteral(hostname: string): boolean {
  const ip = parseIpAddress(hostname);
  return ip !== undefined && BLOCKED_IP_RANGES.has(ip.range());
}

export function normalizeTargetUrl(input: string): string {
  if (Buffer.byteLength(input, 'utf8') > 4096) {
    throw new InvalidRequestError();
  }
  if (/[\s\p{Cc}\p{Cf}]/u.test(input)) {
    throw new UrlNotAllowedError();
  }

  let parsed: URL;
  try {
    parsed = new URL(input);
  } catch {
    throw new UrlNotAllowedError();
  }
  if (!['http:', 'https:'].includes(parsed.protocol) || parsed.hostname === '') {
    throw new UrlNotAllowedError();
  }
  if (parsed.username !== '' || parsed.password !== '') {
    throw new UrlNotAllowedError();
  }
  if (isBlockedHostname(parsed.hostname) || isBlockedIpLiteral(parsed.hostname)) {
    throw new UrlNotAllowedError();
  }
  return parsed.href;
}
