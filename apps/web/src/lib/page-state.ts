import type { Challenge, CreateLinkResponse, ErrorCode } from '@myurl/contracts';

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

export const initialPageState: PageState = { kind: 'idle' };

export type PageEvent =
  | { type: 'submit' }
  | { type: 'success-copied'; result: CreateLinkResponse }
  | { type: 'success-copy-fallback'; result: CreateLinkResponse }
  | { type: 'challenge'; challenge: Challenge; message: string }
  | { type: 'challenge-error'; challenge: Challenge; message: string }
  | { type: 'validation-error'; code: ErrorCode; message: string }
  | { type: 'rate-limited'; code: ErrorCode; message: string; retryAfterSeconds?: number }
  | { type: 'dependency-error'; code: ErrorCode; message: string };

export function reducePageState(_state: PageState, event: PageEvent): PageState {
  switch (event.type) {
    case 'submit':
      return { kind: 'submitting' };
    case 'success-copied':
      return { kind: 'success-copied', result: event.result };
    case 'success-copy-fallback':
      return { kind: 'success-copy-fallback', result: event.result };
    case 'challenge':
      return { kind: 'challenge', challenge: event.challenge, message: event.message };
    case 'challenge-error':
      return { kind: 'challenge-error', challenge: event.challenge, message: event.message };
    case 'validation-error':
      return { kind: 'validation-error', code: event.code, message: event.message };
    case 'rate-limited': {
      const state: PageState = {
        kind: 'rate-limited',
        code: event.code,
        message: event.message,
      };
      if (event.retryAfterSeconds !== undefined) {
        state.retryAfterSeconds = event.retryAfterSeconds;
      }
      return state;
    }
    case 'dependency-error':
      return { kind: 'dependency-error', code: event.code, message: event.message };
  }
}
