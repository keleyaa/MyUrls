import path from 'node:path';
import { fileURLToPath } from 'node:url';

import pino from 'pino';

import { buildApp } from './http.js';
import { parseConfig } from './config.js';
import { RedisLinkStore } from './redis.js';
import { TestTurnstileVerifier, CloudflareTurnstileVerifier } from './turnstile.js';
import { MemoryLinkStore } from './testing/memory-store.js';

const currentDir = path.dirname(fileURLToPath(import.meta.url));

async function main(): Promise<void> {
  const config = parseConfig(process.env);
  const logger = pino({ level: process.env.LOG_LEVEL ?? 'info', base: null });
  const store =
    config.testStore === 'memory'
      ? new MemoryLinkStore()
      : await RedisLinkStore.connect(config.redisUrl, config.redisTimeoutMs);
  const turnstile =
    config.turnstile.mode === 'test'
      ? new TestTurnstileVerifier()
      : new CloudflareTurnstileVerifier(config);
  const webRoot = process.env.WEB_ROOT ?? path.resolve(currentDir, '../../web/dist');
  const app = buildApp({ config, store, turnstile, webRoot, logger });

  let closing = false;
  const close = async (signal: string): Promise<void> => {
    if (closing) {
      return;
    }
    closing = true;
    logger.info({ event: 'server.shutdown.begin', signal }, 'shutdown requested');
    try {
      await Promise.race([
        app.close(),
        new Promise<never>((_, reject) =>
          setTimeout(() => reject(new Error('shutdown timeout')), config.shutdownTimeoutMs),
        ),
      ]);
      logger.info({ event: 'server.shutdown.complete' }, 'shutdown complete');
      process.exit(0);
    } catch {
      logger.error({ event: 'server.shutdown.failed' }, 'shutdown failed');
      process.exit(1);
    }
  };
  process.once('SIGTERM', () => void close('SIGTERM'));
  process.once('SIGINT', () => void close('SIGINT'));

  await app.listen({ host: '0.0.0.0', port: config.port });
  logger.info(
    {
      event: 'server.started',
      port: config.port,
      nodeEnv: config.nodeEnv,
      turnstileEnabled: config.turnstile.enabled,
      trustedProxyCount: config.trustProxyCidrs.length,
    },
    'server started',
  );
}

main().catch((error: unknown) => {
  const logger = pino({ level: 'error', base: null });
  logger.error({ event: 'server.start.failed' }, 'server failed to start');
  process.exitCode = 1;
  void error;
});
