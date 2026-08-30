import type { Challenge, CreateLinkResponse, ErrorCode } from './api.js';

export type PageState =
  | { kind: 'idle' }
  | { kind: 'submitting' }
  | { kind: 'success-copied'; result: CreateLinkResponse }
  | { kind: 'success-copy-fallback'; result: CreateLinkResponse }
  | { kind: 'challenge'; challenge: Challenge; message: string }
  | { kind: 'challenge-error'; challenge: Challenge; message: string }
  | { kind: 'validation-error'; code: ErrorCode; message: string }
  | { kind: 'rate-limited'; code: ErrorCode; message: string; retryAfterSeconds?: number }
  | { kind: 'dependency-error'; code: ErrorCode; message: string };
