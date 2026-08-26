import { Type, type Static } from '@sinclair/typebox';

export const CreateLinkBodySchema = Type.Object(
  {
    url: Type.String({ minLength: 1, maxLength: 4096 }),
    alias: Type.Optional(Type.String({ maxLength: 64 })),
    challengeToken: Type.Optional(Type.String({ maxLength: 2048 })),
  },
  { additionalProperties: false, maxProperties: 3 },
);

export type CreateLinkInput = Static<typeof CreateLinkBodySchema>;

export const CreateLinkResponseSchema = Type.Object({
  code: Type.String({ minLength: 4, maxLength: 32 }),
  shortUrl: Type.String({ minLength: 1 }),
  expiresAt: Type.String({ minLength: 1 }),
});

export type CreateLinkResponse = Static<typeof CreateLinkResponseSchema>;

export const ErrorCodeSchema = Type.Union([
  Type.Literal('invalid_request'),
  Type.Literal('challenge_required'),
  Type.Literal('challenge_invalid'),
  Type.Literal('alias_unavailable'),
  Type.Literal('url_not_allowed'),
  Type.Literal('alias_invalid'),
  Type.Literal('rate_limited'),
  Type.Literal('dependency_unavailable'),
  Type.Literal('code_generation_exhausted'),
]);

export type ErrorCode = Static<typeof ErrorCodeSchema>;

export const ErrorDetailSchema = Type.Object({
  code: ErrorCodeSchema,
  requestId: Type.String({ minLength: 1, maxLength: 80 }),
  retryAfterSeconds: Type.Optional(Type.Integer({ minimum: 1 })),
});

export const ErrorResponseSchema = Type.Object({
  error: ErrorDetailSchema,
  challenge: Type.Optional(
    Type.Object({
      provider: Type.Literal('turnstile'),
      siteKey: Type.String({ minLength: 1 }),
    }),
  ),
});

export type ErrorResponse = Static<typeof ErrorResponseSchema>;

export const ChallengeSchema = Type.Object({
  provider: Type.Literal('turnstile'),
  siteKey: Type.String({ minLength: 1 }),
});

export type Challenge = Static<typeof ChallengeSchema>;
