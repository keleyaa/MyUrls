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

function isAscii(value: string): boolean {
  return [...value].every((character) => character.charCodeAt(0) <= 0x7f);
}

export function normalizeAlias(value: string): string {
  const trimmed = value.trim();
  if (!isAscii(trimmed)) {
    throw new AliasInvalidError();
  }
  const normalized = trimmed.toLowerCase();
  if (!ALIAS_PATTERN.test(normalized)) {
    throw new AliasInvalidError();
  }
  return normalized;
}

export function isReservedCode(value: string): boolean {
  return RESERVED_CODES.has(value.toLowerCase());
}
