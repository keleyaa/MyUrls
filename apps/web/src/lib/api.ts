import { isCreateLinkResponse } from '@myurl/contracts';
import type {
  Challenge,
  CreateLinkInput,
  CreateLinkResponse,
  ErrorCode,
  ProblemDetails,
} from '@myurl/contracts';

const errorCodes = new Set<ErrorCode>([
  'invalid_request',
  'challenge_required',
  'challenge_invalid',
  'alias_unavailable',
  'url_not_allowed',
  'alias_invalid',
  'rate_limited',
  'dependency_unavailable',
  'code_generation_exhausted',
]);

const dependencyUnavailable: ProblemDetails = {
  type: 'urn:myurl:client:dependency-unavailable',
  title: 'Service unavailable',
  status: 503,
  code: 'dependency_unavailable',
  requestId: 'client',
};

export class ApiError extends Error {
  readonly status: number;
  readonly code: ErrorCode;
  readonly challenge?: Challenge;
  readonly retryAfterSeconds?: number;

  constructor(problem: ProblemDetails) {
    super(problem.code);
    this.name = 'ApiError';
    this.status = problem.status;
    this.code = problem.code;
    if (problem.challenge !== undefined) {
      this.challenge = problem.challenge;
    }
    if (problem.retryAfterSeconds !== undefined) {
      this.retryAfterSeconds = problem.retryAfterSeconds;
    }
  }
}

function isChallenge(value: unknown): value is Challenge {
  if (typeof value !== 'object' || value === null) {
    return false;
  }
  const candidate = value as { provider?: unknown; siteKey?: unknown };
  return (
    candidate.provider === 'turnstile' &&
    typeof candidate.siteKey === 'string' &&
    candidate.siteKey.length > 0
  );
}

function isProblemDetails(value: unknown): value is ProblemDetails {
  if (typeof value !== 'object' || value === null) {
    return false;
  }

  const candidate = value as Record<string, unknown>;
  if (
    typeof candidate.type !== 'string' ||
    candidate.type.length === 0 ||
    typeof candidate.title !== 'string' ||
    candidate.title.length === 0 ||
    typeof candidate.status !== 'number' ||
    !Number.isInteger(candidate.status) ||
    candidate.status < 400 ||
    candidate.status > 599 ||
    typeof candidate.code !== 'string' ||
    !errorCodes.has(candidate.code as ErrorCode) ||
    typeof candidate.requestId !== 'string' ||
    candidate.requestId.length === 0 ||
    candidate.requestId.length > 80
  ) {
    return false;
  }

  if (
    (candidate.retryAfterSeconds !== undefined &&
      (typeof candidate.retryAfterSeconds !== 'number' ||
        !Number.isInteger(candidate.retryAfterSeconds) ||
        candidate.retryAfterSeconds < 1)) ||
    (candidate.challenge !== undefined && !isChallenge(candidate.challenge))
  ) {
    return false;
  }

  return true;
}

async function readJson(response: Response): Promise<unknown> {
  try {
    return (await response.json()) as unknown;
  } catch {
    return undefined;
  }
}

export async function createLink(input: CreateLinkInput): Promise<CreateLinkResponse> {
  let response: Response;
  try {
    response = await fetch('/api/links', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(input),
    });
  } catch {
    throw new ApiError(dependencyUnavailable);
  }

  const payload = await readJson(response);
  if (!response.ok) {
    if (isProblemDetails(payload)) {
      throw new ApiError(payload);
    }
    throw new ApiError(dependencyUnavailable);
  }

  if (isCreateLinkResponse(payload)) {
    return payload;
  }

  throw new ApiError(dependencyUnavailable);
}

export async function checkReady(): Promise<boolean> {
  try {
    const response = await fetch('/health/ready', { cache: 'no-store' });
    return response.ok;
  } catch {
    return false;
  }
}
