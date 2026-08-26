import { describe, expect, it } from 'vitest';

import { AliasInvalidError } from '../errors.js';
import { isReservedCode, normalizeAlias } from './alias-policy.js';

describe('alias policy', () => {
  it('trims and lowercases an ASCII alias', () => {
    expect(normalizeAlias('  Launch_42  ')).toBe('launch_42');
  });

  it.each([
    'abc',
    'a'.repeat(33),
    'hello.world',
    'hello world',
    'аlias',
    'Kaunch',
    'foo/bar',
    'foo%2Fbar',
  ])(
    'rejects invalid alias %s',
    (value) => {
      expect(() => normalizeAlias(value)).toThrow(AliasInvalidError);
    },
  );

  it('recognizes reserved paths case-insensitively', () => {
    expect(isReservedCode('API')).toBe(true);
    expect(isReservedCode('favicon.ico')).toBe(true);
    expect(isReservedCode('launch')).toBe(false);
  });
});
