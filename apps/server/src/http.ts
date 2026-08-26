import { existsSync } from 'node:fs';
import { randomUUID } from 'node:crypto';

import fastifyStatic from '@fastify/static';
import { type TypeBoxTypeProvider } from '@fastify/type-provider-typebox';
import Fastify, { LogController, type FastifyReply, type FastifyRequest } from 'fastify';
import type { CreateLinkInput } from '@myurl/contracts';
import type { Logger } from 'pino';

import {
  CreateLinkBodySchema,
  CreateLinkResponseSchema,
  ErrorResponseSchema,
} from '@myurl/contracts';
import { MAX_BODY_BYTES, type AppConfig } from './config.js';
import {
  DependencyUnavailableError,
  InvalidRequestError,
  isMyUrlError,
  toErrorResponse,
} from './errors.js';
import type { MyUrlError } from './errors.js';
import { getClientIp } from './ip.js';
import type { LinkStore, TurnstileVerifier } from './ports.js';
import { ShortLinkService } from './service.js';

export interface BuildAppOptions {
  config: AppConfig;
  store: LinkStore;
  turnstile: TurnstileVerifier;
  webRoot: string;
  logger: Logger;
  service?: ShortLinkService;
}

interface RequestErrorShape {
  code?: unknown;
  validation?: unknown;
}

function isRequestErrorShape(error: unknown): error is RequestErrorShape {
  return typeof error === 'object' && error !== null;
}

function requestId(): string {
  return `req_${randomUUID().replaceAll('-', '')}`;
}

function pathOnly(url: string): string {
  const questionMark = url.indexOf('?');
  return questionMark === -1 ? url : url.slice(0, questionMark);
}

function isCreateRoute(request: FastifyRequest): boolean {
  return request.method === 'POST' && pathOnly(request.url) === '/api/v1/links';
}

function isJsonContentType(value: string | undefined): boolean {
  return value !== undefined && /^application\/json(?:\s*;|$)/i.test(value);
}

function errorPage(statusCode: 404 | 503): string {
  const title = statusCode === 404 ? 'Link not found' : 'Service unavailable';
  const message =
    statusCode === 404
      ? 'This short link is unavailable or has expired.'
      : 'The service is temporarily unable to complete this request.';
  return `<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>${title} · myurl</title>
    <style>
      :root { color-scheme: dark; font-family: system-ui, sans-serif; background: #0d1110; color: #e8eee9; }
      body { margin: 0; min-height: 100vh; display: grid; place-items: center; padding: 24px; box-sizing: border-box; }
      main { width: min(100%, 520px); border: 1px solid #2b3731; border-radius: 8px; padding: 28px; background: #151b18; }
      strong { color: #62d88b; font: 600 18px ui-monospace, monospace; }
      h1 { margin: 36px 0 10px; font-size: 36px; }
      p { margin: 0; color: #aab8ae; line-height: 1.6; }
    </style>
  </head>
  <body><main><strong>myurl</strong><h1>${title}</h1><p>${message}</p></main></body>
</html>`;
}

function sendBrowserError(
  reply: FastifyReply,
  statusCode: 404 | 503,
  headOnly: boolean,
): FastifyReply {
  reply.code(statusCode).type('text/html; charset=utf-8');
  return headOnly ? reply.send() : reply.send(errorPage(statusCode));
}

function sendJsonError(
  reply: FastifyReply,
  error: MyUrlError,
  requestIdValue: string,
): FastifyReply {
  reply.code(error.statusCode).type('application/json; charset=utf-8');
  if (error.retryAfterSeconds !== undefined) {
    reply.header('Retry-After', String(error.retryAfterSeconds));
  }
  return reply.send(toErrorResponse(error, requestIdValue));
}

function sendMissingApi(reply: FastifyReply, requestIdValue: string): FastifyReply {
  const error = new InvalidRequestError();
  reply.code(404).type('application/json; charset=utf-8');
  return reply.send(toErrorResponse(error, requestIdValue));
}

export function buildApp(options: BuildAppOptions) {
  const { config, store, turnstile, webRoot, logger } = options;
  const service = options.service ?? new ShortLinkService({ config, store, turnstile });
  const app = Fastify({
    bodyLimit: MAX_BODY_BYTES,
    loggerInstance: logger,
    logController: new LogController({ disableRequestLogging: true }),
    ajv: { customOptions: { removeAdditional: false } },
    exposeHeadRoutes: false,
    requestTimeout: config.requestTimeoutMs,
    genReqId: (request) => {
      const supplied = request.headers['x-request-id'] ?? request.headers['request-id'];
      if (typeof supplied === 'string' && /^[A-Za-z0-9_-]{8,80}$/.test(supplied)) {
        return supplied;
      }
      return requestId();
    },
  }).withTypeProvider<TypeBoxTypeProvider>();

  app.addHook('onRequest', async (request) => {
    if (!isCreateRoute(request)) {
      return;
    }
    const contentType = request.headers['content-type'];
    if (typeof contentType !== 'string' || !isJsonContentType(contentType)) {
      throw new InvalidRequestError();
    }
    const origin = request.headers.origin;
    if (origin !== undefined) {
      if (typeof origin !== 'string' || origin !== config.publicBaseOrigin) {
        throw new InvalidRequestError();
      }
    }
  });

  app.addHook('onSend', async (_request, reply, payload) => {
    reply.header('Cache-Control', 'no-store');
    reply.header(
      'Content-Security-Policy',
      "default-src 'self'; base-uri 'none'; object-src 'none'; frame-ancestors 'none'; form-action 'self'; script-src 'self' https://challenges.cloudflare.com; style-src 'self'; font-src 'self'; img-src 'self'; connect-src 'self' https://challenges.cloudflare.com; frame-src https://challenges.cloudflare.com",
    );
    reply.header('Permissions-Policy', 'camera=(), microphone=(), geolocation=()');
    reply.header('Referrer-Policy', 'no-referrer');
    reply.header('X-Content-Type-Options', 'nosniff');
    reply.header('X-Frame-Options', 'DENY');
    reply.header('X-Robots-Tag', 'noindex, nofollow');
    return payload;
  });

  app.addHook('onResponse', async (request, reply) => {
    const route =
      typeof request.routeOptions.url === 'string' ? request.routeOptions.url : '/not-found';
    request.log.info(
      {
        event: 'request.complete',
        route,
        statusCode: reply.statusCode,
        durationMs: Number(reply.elapsedTime.toFixed(2)),
      },
      'request complete',
    );
  });

  app.addHook('onClose', async () => {
    await store.close();
  });

  const hasWebRoot = existsSync(webRoot);
  if (hasWebRoot) {
    void app.register(fastifyStatic, {
      root: webRoot,
      prefix: '/',
      wildcard: false,
    });
  }

  if (!hasWebRoot) {
    app.get('/', async (_request, reply) => {
      return sendBrowserError(reply, 503, false);
    });
    for (const publicFile of ['favicon.ico', 'robots.txt', 'sitemap.xml']) {
      app.get(`/${publicFile}`, async (_request, reply) => {
        return reply.code(404).send();
      });
    }
  }

  app.get('/health/live', async (_request, reply) => reply.code(200).send({ status: 'ok' }));

  app.get('/health/ready', async (_request, reply) => {
    try {
      await store.ping();
      return reply.code(200).send({ status: 'ok' });
    } catch {
      return reply.code(503).send({ status: 'degraded' });
    }
  });

  app.post(
    '/api/v1/links',
    {
      schema: {
        body: CreateLinkBodySchema,
        response: {
          201: CreateLinkResponseSchema,
          400: ErrorResponseSchema,
          403: ErrorResponseSchema,
          409: ErrorResponseSchema,
          422: ErrorResponseSchema,
          429: ErrorResponseSchema,
          503: ErrorResponseSchema,
        },
      },
    },
    async (request, reply) => {
      const input = request.body as CreateLinkInput;
      const clientIp = getClientIp(
        request.raw.socket.remoteAddress,
        {
          'x-forwarded-for':
            typeof request.headers['x-forwarded-for'] === 'string'
              ? request.headers['x-forwarded-for']
              : undefined,
          forwarded:
            typeof request.headers.forwarded === 'string' ? request.headers.forwarded : undefined,
        },
        config.trustProxyCidrs,
      );
      const result = await service.create(input, { clientIp });
      request.log.info({ event: 'link.create', result: 'success' }, 'link created');
      return reply.code(201).send(result);
    },
  );

  const resolveRedirect = async (
    request: FastifyRequest<{ Params: { code: string } }>,
    reply: FastifyReply,
  ) => {
    const targetUrl = await service.resolve(request.params.code);
    if (targetUrl === undefined) {
      return sendBrowserError(reply, 404, request.method === 'HEAD');
    }
    reply
      .code(302)
      .header('Location', targetUrl)
      .header('Cache-Control', 'no-store')
      .header('Referrer-Policy', 'no-referrer')
      .header('X-Robots-Tag', 'noindex, nofollow');
    return reply.send();
  };

  app.get(
    '/:code',
    {
      schema: {
        params: { type: 'object', properties: { code: { type: 'string' } }, required: ['code'] },
      },
    },
    resolveRedirect,
  );
  app.head(
    '/:code',
    {
      schema: {
        params: { type: 'object', properties: { code: { type: 'string' } }, required: ['code'] },
      },
    },
    resolveRedirect,
  );

  app.setNotFoundHandler(async (request, reply) => {
    if (pathOnly(request.url).startsWith('/api')) {
      return sendMissingApi(reply, request.id);
    }
    return sendBrowserError(reply, 404, request.method === 'HEAD');
  });

  app.setErrorHandler(async (error, request, reply) => {
    const isRedirectRequest =
      (request.method === 'GET' || request.method === 'HEAD') &&
      request.routeOptions.url === '/:code';
    if (isRedirectRequest) {
      return sendBrowserError(reply, 503, request.method === 'HEAD');
    }
    if (isMyUrlError(error)) {
      request.log.info({ event: 'request.rejected', errorCode: error.code }, 'request rejected');
      return sendJsonError(reply, error, request.id);
    }

    const errorShape = isRequestErrorShape(error) ? error : {};
    if (
      errorShape.validation !== undefined ||
      errorShape.code === 'FST_ERR_CTP_INVALID_JSON_BODY' ||
      errorShape.code === 'FST_ERR_CTP_BODY_TOO_LARGE' ||
      errorShape.code === 'FST_ERR_CTP_INVALID_MEDIA_TYPE'
    ) {
      const invalid = new InvalidRequestError();
      request.log.info({ event: 'request.rejected', errorCode: invalid.code }, 'request rejected');
      return sendJsonError(reply, invalid, request.id);
    }

    const dependency = new DependencyUnavailableError();
    request.log.error({ event: 'request.failed', errorCode: dependency.code }, 'request failed');
    return sendJsonError(reply, dependency, request.id);
  });

  return app;
}
