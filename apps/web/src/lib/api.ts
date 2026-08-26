import type {
  Challenge,
  CreateLinkInput,
  CreateLinkResponse,
  ErrorCode,
  ErrorResponse,
} from '@myurl/contracts';

export class ApiError extends Error {
  readonly status: number;
  readonly code: ErrorCode;
  readonly challenge?: Challenge;
  readonly retryAfterSeconds?: number;

  constructor(status: number, response: ErrorResponse) {
    super(response.error.code);
    this.name = 'ApiError';
    this.status = status;
    this.code = response.error.code;
    if (response.challenge !== undefined) {
      this.challenge = response.challenge;
    }
    if (response.error.retryAfterSeconds !== undefined) {
      this.retryAfterSeconds = response.error.retryAfterSeconds;
    }
  }
}

function isErrorResponse(value: unknown): value is ErrorResponse {
  if (typeof value !== 'object' || value === null || !('error' in value)) {
    return false;
  }
  const error = (value as { error?: unknown }).error;
  return typeof error === 'object' && error !== null && 'code' in error && 'requestId' in error;
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
    response = await fetch('/api/v1/links', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(input),
    });
  } catch {
    throw new ApiError(503, {
      error: { code: 'dependency_unavailable', requestId: 'client' },
    });
  }
  const payload = await readJson(response);
  if (!response.ok) {
    if (isErrorResponse(payload)) {
      throw new ApiError(response.status, payload);
    }
    throw new ApiError(503, {
      error: { code: 'dependency_unavailable', requestId: 'client' },
    });
  }
  return payload as CreateLinkResponse;
}

export async function checkReady(): Promise<boolean> {
  try {
    const response = await fetch('/health/ready', { cache: 'no-store' });
    return response.ok;
  } catch {
    return false;
  }
}
