import type { Challenge, ErrorCode, ErrorResponse } from '@myurl/contracts';

export class MyUrlError extends Error {
  readonly statusCode: number;
  readonly code: ErrorCode;
  readonly retryAfterSeconds?: number;
  readonly challenge?: Challenge;

  constructor(
    code: ErrorCode,
    statusCode: number,
    options?: { retryAfterSeconds?: number; challenge?: Challenge },
  ) {
    super(code);
    this.name = 'MyUrlError';
    this.statusCode = statusCode;
    this.code = code;
    if (options?.retryAfterSeconds !== undefined) {
      this.retryAfterSeconds = options.retryAfterSeconds;
    }
    if (options?.challenge !== undefined) {
      this.challenge = options.challenge;
    }
  }
}

export class InvalidRequestError extends MyUrlError {
  constructor() {
    super('invalid_request', 400);
  }
}

export class ChallengeRequiredError extends MyUrlError {
  constructor(siteKey: string) {
    super('challenge_required', 403, {
      challenge: { provider: 'turnstile', siteKey },
    });
  }
}

export class ChallengeInvalidError extends MyUrlError {
  constructor(siteKey: string) {
    super('challenge_invalid', 403, {
      challenge: { provider: 'turnstile', siteKey },
    });
  }
}

export class AliasUnavailableError extends MyUrlError {
  constructor() {
    super('alias_unavailable', 409);
  }
}

export class UrlNotAllowedError extends MyUrlError {
  constructor() {
    super('url_not_allowed', 422);
  }
}

export class AliasInvalidError extends MyUrlError {
  constructor() {
    super('alias_invalid', 422);
  }
}

export class RateLimitedError extends MyUrlError {
  constructor(retryAfterSeconds = 120) {
    super('rate_limited', 429, { retryAfterSeconds });
  }
}

export class DependencyUnavailableError extends MyUrlError {
  constructor() {
    super('dependency_unavailable', 503);
  }
}

export class CodeGenerationExhaustedError extends MyUrlError {
  constructor() {
    super('code_generation_exhausted', 503);
  }
}

export function toErrorResponse(error: MyUrlError, requestId: string): ErrorResponse {
  const detail: ErrorResponse['error'] = {
    code: error.code,
    requestId,
  };
  if (error.retryAfterSeconds !== undefined) {
    detail.retryAfterSeconds = error.retryAfterSeconds;
  }

  const response: ErrorResponse = { error: detail };
  if (error.challenge !== undefined) {
    response.challenge = error.challenge;
  }
  return response;
}

export function isMyUrlError(error: unknown): error is MyUrlError {
  return error instanceof MyUrlError;
}
