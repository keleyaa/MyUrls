import { createClient, type RedisClientType } from 'redis';

import { DependencyUnavailableError } from './errors.js';
import type { LinkStore } from './ports.js';

const COUNTER_SCRIPT = `
local short_count = redis.call('INCR', KEYS[1])
if short_count == 1 then redis.call('EXPIRE', KEYS[1], ARGV[1]) end
local daily_count = redis.call('INCR', KEYS[2])
if daily_count == 1 then redis.call('EXPIRE', KEYS[2], ARGV[2]) end
return { short_count, daily_count }
`;

const SINGLE_COUNTER_SCRIPT = `
local count = redis.call('INCR', KEYS[1])
if count == 1 then redis.call('EXPIRE', KEYS[1], ARGV[1]) end
return count
`;

const RISK_SCRIPT = `
local existed = redis.call('EXISTS', KEYS[1])
local score = redis.call('INCRBY', KEYS[1], ARGV[1])
if existed == 0 then redis.call('EXPIRE', KEYS[1], ARGV[2]) end
return score
`;

const DAILY_COUNTER_TTL_SECONDS = 172800;
const RESOLVE_COUNTER_TTL_SECONDS = 10;
const RISK_TTL_SECONDS = 600;

function withTimeout<T>(operation: Promise<T>, timeoutMs: number): Promise<T> {
  let timeout: NodeJS.Timeout | undefined;
  const timer = new Promise<never>((_, reject) => {
    timeout = setTimeout(() => reject(new Error('operation timed out')), timeoutMs);
  });
  return Promise.race([operation, timer]).finally(() => {
    if (timeout !== undefined) {
      clearTimeout(timeout);
    }
  });
}

export class RedisLinkStore implements LinkStore {
  private constructor(
    private readonly client: RedisClientType,
    private readonly timeoutMs: number,
  ) {}

  static async connect(redisUrl: string, timeoutMs: number): Promise<RedisLinkStore> {
    const client = createClient({
      url: redisUrl,
      socket: {
        connectTimeout: timeoutMs,
        reconnectStrategy: () => false,
      },
    });
    client.on('error', () => undefined);
    try {
      await withTimeout(client.connect(), timeoutMs);
    } catch {
      client.destroy();
      throw new DependencyUnavailableError();
    }
    return new RedisLinkStore(client, timeoutMs);
  }

  async claim(code: string, targetUrl: string, ttlSeconds: number): Promise<boolean> {
    return this.command(async () => {
      const result = await this.client.set(`myurl:link:${code}`, targetUrl, {
        NX: true,
        EX: ttlSeconds,
      });
      return result === 'OK';
    });
  }

  async lookup(code: string): Promise<string | undefined> {
    return this.command(async () => {
      const result = await this.client.get(`myurl:link:${code}`);
      return result === null ? undefined : result;
    });
  }

  async incrementResolveCounter(fingerprint: string): Promise<number> {
    return this.command(async () => {
      const result = await this.client.eval(SINGLE_COUNTER_SCRIPT, {
        keys: [`myurl:rate:resolve:10s:${fingerprint}`],
        arguments: [String(RESOLVE_COUNTER_TTL_SECONDS)],
      });
      const count = Number(result);
      if (!Number.isSafeInteger(count) || count < 1) {
        throw new Error('invalid resolve counter result');
      }
      return count;
    });
  }

  async incrementCreateCounters(
    fingerprint: string,
    utcDate: string,
  ): Promise<{ tenMinuteCount: number; dailyCount: number }> {
    return this.command(async () => {
      const result = await this.client.eval(COUNTER_SCRIPT, {
        keys: [
          `myurl:rate:create:10m:${fingerprint}`,
          `myurl:rate:create:1d:${utcDate}:${fingerprint}`,
        ],
        arguments: ['600', String(DAILY_COUNTER_TTL_SECONDS)],
      });
      if (!Array.isArray(result) || result.length !== 2) {
        throw new Error('invalid counter result');
      }
      const tenMinuteCount = Number(result[0]);
      const dailyCount = Number(result[1]);
      if (!Number.isSafeInteger(tenMinuteCount) || !Number.isSafeInteger(dailyCount)) {
        throw new Error('invalid counter result');
      }
      return { tenMinuteCount, dailyCount };
    });
  }

  async getRiskScore(fingerprint: string): Promise<number> {
    return this.command(async () => {
      const value = await this.client.get(`myurl:risk:create:10m:${fingerprint}`);
      if (value === null) {
        return 0;
      }
      const score = Number(value);
      if (!Number.isSafeInteger(score) || score < 0) {
        throw new Error('invalid risk score');
      }
      return score;
    });
  }

  async addRiskScore(fingerprint: string, points: number): Promise<number> {
    return this.command(async () => {
      const result = await this.client.eval(RISK_SCRIPT, {
        keys: [`myurl:risk:create:10m:${fingerprint}`],
        arguments: [String(points), String(RISK_TTL_SECONDS)],
      });
      const score = Number(result);
      if (!Number.isSafeInteger(score) || score < 0) {
        throw new Error('invalid risk result');
      }
      return score;
    });
  }

  async ping(): Promise<void> {
    await this.command(async () => {
      const result = await this.client.ping();
      if (result !== 'PONG') {
        throw new Error('unexpected ping result');
      }
    });
  }

  async close(): Promise<void> {
    if (!this.client.isOpen) {
      return;
    }
    try {
      await withTimeout(this.client.quit(), this.timeoutMs);
    } catch {
      this.client.destroy();
    }
  }

  private async command<T>(operation: () => Promise<T>): Promise<T> {
    try {
      return await withTimeout(operation(), this.timeoutMs);
    } catch {
      throw new DependencyUnavailableError();
    }
  }
}
