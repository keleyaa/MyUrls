import { AliasInvalidError } from '../errors.js';

export const RESERVED_CODES = new Set([
  'api',
  'assets',
  'health',
  'favicon.ico',
  'robots.txt',
  'sitemap.xml',
]);

const ALIAS_PATTERN = /^[a-z0-9_-]{4,32}$/;

export function normalizeAlias(value: string): string {
  const normalized = value.trim().toLowerCase();
  if (!ALIAS_PATTERN.test(normalized)) {
    throw new AliasInvalidError();
  }
  return normalized;
}

export function isReservedCode(value: string): boolean {
  return RESERVED_CODES.has(value.toLowerCase());
}
