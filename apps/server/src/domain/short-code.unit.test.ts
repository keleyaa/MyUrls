import { describe, expect, it } from 'vitest';

import { BASE62, generateShortCode, isValidCode } from './short-code.js';

describe('short code generation', () => {
  it('generates a fixed length Base62 code', () => {
    const code = generateShortCode(() => new Uint8Array(32).fill(0));
    expect(code).toBe('00000000');
    expect(code).toHaveLength(8);
    expect([...code].every((character) => BASE62.includes(character))).toBe(true);
    expect(generateShortCode()).toMatch(/^[0-9A-Za-z]{8}$/);
  });

  it('rejects invalid code shapes', () => {
    expect(isValidCode('abcd')).toBe(true);
    expect(isValidCode('ab')).toBe(false);
    expect(isValidCode('hello.world')).toBe(false);
    expect(isValidCode('a'.repeat(33))).toBe(false);
  });
});
