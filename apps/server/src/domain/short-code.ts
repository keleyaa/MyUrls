import { randomBytes } from 'node:crypto';

import { AUTO_CODE_LENGTH } from '../config.js';

export const BASE62 = '0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz';
export const CODE_PATTERN = /^[0-9A-Za-z_-]{4,32}$/;

export type RandomByteSource = (size: number) => Uint8Array;

function secureRandomBytes(size: number): Uint8Array {
  return randomBytes(size);
}

export function generateShortCode(
  source: RandomByteSource = secureRandomBytes,
  length = AUTO_CODE_LENGTH,
): string {
  let code = '';
  while (code.length < length) {
    const bytes = source(Math.max(16, (length - code.length) * 2));
    for (const byte of bytes) {
      if (byte < 248) {
        code += BASE62.charAt(byte % BASE62.length);
        if (code.length === length) {
          break;
        }
      }
    }
  }
  return code;
}

export function isValidCode(value: string): boolean {
  return CODE_PATTERN.test(value);
}
