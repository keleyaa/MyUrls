import type { AppConfig } from './config.js';
import { DependencyUnavailableError } from './errors.js';
import type { TurnstileVerifier } from './ports.js';

interface TurnstileResponse {
  success?: unknown;
  hostname?: unknown;
  action?: unknown;
}

function isTurnstileResponse(value: unknown): value is TurnstileResponse {
  return typeof value === 'object' && value !== null;
}

export class CloudflareTurnstileVerifier implements TurnstileVerifier {
  constructor(private readonly config: AppConfig) {}

  async verify(token: string): Promise<{ valid: boolean }> {
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), this.config.turnstileTimeoutMs);
    try {
      const body = new URLSearchParams({
        secret: this.config.turnstile.secretKey,
        response: token,
      });
      const response = await fetch('https://challenges.cloudflare.com/turnstile/v0/siteverify', {
        method: 'POST',
        headers: { 'content-type': 'application/x-www-form-urlencoded' },
        body,
        signal: controller.signal,
      });
      if (!response.ok) {
        throw new DependencyUnavailableError();
      }
      const payload: unknown = await response.json();
      if (!isTurnstileResponse(payload)) {
        throw new DependencyUnavailableError();
      }
      if (payload.success !== true) {
        return { valid: false };
      }
      if (
        this.config.nodeEnv === 'production' &&
        (payload.hostname !== this.config.turnstile.hostname || payload.action !== 'create_link')
      ) {
        return { valid: false };
      }
      return { valid: true };
    } catch (error) {
      if (error instanceof DependencyUnavailableError) {
        throw error;
      }
      throw new DependencyUnavailableError();
    } finally {
      clearTimeout(timeout);
    }
  }
}

export class TestTurnstileVerifier implements TurnstileVerifier {
  async verify(token: string): Promise<{ valid: boolean }> {
    return { valid: token === 'test-token' };
  }
}
