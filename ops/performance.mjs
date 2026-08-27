import { createServer } from 'node:net';
import { spawn } from 'node:child_process';
import { request as httpRequest, Agent } from 'node:http';
import { Buffer } from 'node:buffer';
import { performance } from 'node:perf_hooks';
import { URL } from 'node:url';

const httpAgent = new Agent({ keepAlive: true, maxSockets: 100, maxFreeSockets: 100 });

function freePort() {
  return new Promise((resolve, reject) => {
    const server = createServer();
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => {
      const address = server.address();
      if (address === null || typeof address === 'string') {
        server.close(() => reject(new Error('Could not select a port')));
        return;
      }
      server.close((error) => (error ? reject(error) : resolve(address.port)));
    });
  });
}

function run(command, args, env) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { env, stdio: 'inherit' });
    child.once('error', reject);
    child.once('exit', (code, signal) => {
      if (code === 0) resolve();
      else reject(new Error(`${command} exited with ${signal ?? code}`));
    });
  });
}

function percentile(values, fraction) {
  if (values.length === 0) return null;
  const sorted = [...values].sort((a, b) => a - b);
  const index = Math.min(sorted.length - 1, Math.ceil(sorted.length * fraction) - 1);
  return Number(sorted[index].toFixed(2));
}

function sleep(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

function requestHttp(url, { method = 'GET', headers = {}, body, timeoutMs = 5000 } = {}) {
  return new Promise((resolve, reject) => {
    const request = httpRequest(
      new URL(url),
      { method, headers, agent: httpAgent, timeout: timeoutMs },
      (response) => resolve(response),
    );
    request.once('timeout', () => request.destroy(new Error('request timed out')));
    request.once('error', reject);
    if (body !== undefined) {
      request.write(body);
    }
    request.end();
  });
}

function consumeResponse(response) {
  return new Promise((resolve, reject) => {
    const chunks = [];
    response.on('data', (chunk) => chunks.push(chunk));
    response.once('end', () => resolve(Buffer.concat(chunks)));
    response.once('error', reject);
  });
}

async function measure({ baseUrl, durationMs, concurrency, expectedStatus, request }) {
  const latencies = [];
  let total = 0;
  let failures = 0;
  const deadline = performance.now() + durationMs;

  async function worker(workerId) {
    while (performance.now() < deadline) {
      const started = performance.now();
      const sequence = total;
      total += 1;
      try {
        const response = await request({ baseUrl, workerId, sequence });
        const status = response.statusCode ?? 0;
        await consumeResponse(response);
        if (status !== expectedStatus) failures += 1;
        if (status === expectedStatus) latencies.push(performance.now() - started);
      } catch {
        failures += 1;
      }
    }
  }

  await Promise.all(Array.from({ length: concurrency }, (_, index) => worker(index)));
  return {
    total,
    failures,
    errorRate: total === 0 ? 0 : failures / total,
    p50Ms: percentile(latencies, 0.5),
    p95Ms: percentile(latencies, 0.95),
  };
}

const warmupSeconds = Number(process.env.PERF_WARMUP_SECONDS ?? '30');
const durationSeconds = Number(process.env.PERF_DURATION_SECONDS ?? '60');
if (!Number.isInteger(warmupSeconds) || warmupSeconds < 0) {
  throw new Error('PERF_WARMUP_SECONDS must be a non-negative integer');
}
if (!Number.isInteger(durationSeconds) || durationSeconds <= 0) {
  throw new Error('PERF_DURATION_SECONDS must be a positive integer');
}

const project = `myurl-v2-performance-${process.pid}`;
const port = await freePort();
const baseUrl = `http://127.0.0.1:${port}`;
const composeArgs = ['-f', 'docker-compose.yaml', '-p', project];
const env = {
  ...process.env,
  APP_PORT: String(port),
  PUBLIC_BASE_URL: baseUrl,
  NODE_ENV: 'development',
  LOG_LEVEL: 'warn',
  IP_HASH_SECRET: 'performance-secret-that-is-at-least-32-bytes-long',
  REDIS_URL: 'redis://redis:6379/0',
  REDIS_PASSWORD: 'performance-redis-password',
  TRUST_PROXY_CIDRS: '172.16.0.0/12',
  TURNSTILE_ENABLED: 'false',
  CREATE_DIRECT_LIMIT_10M: '100000',
  CREATE_HARD_LIMIT_10M: '200000',
  CREATE_HARD_LIMIT_1D: '300000',
  RESOLVE_LIMIT_10S: '1000000',
};

try {
  await run('docker', ['compose', ...composeArgs, 'up', '-d', '--build', '--wait'], env);
  const live = await fetch(`${baseUrl}/health/live`);
  const ready = await fetch(`${baseUrl}/health/ready`);
  if (!live.ok || !ready.ok) throw new Error('performance stack health checks failed');

  const seedBody = JSON.stringify({ url: 'https://example.com/performance-seed' });
  const seedResponse = await requestHttp(`${baseUrl}/api/v1/links`, {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
      'content-length': String(Buffer.byteLength(seedBody)),
      'x-forwarded-for': '198.51.100.250',
    },
    body: seedBody,
  });
  const seedResponseBody = await consumeResponse(seedResponse);
  if (seedResponse.statusCode !== 201)
    throw new Error(`performance seed failed: ${seedResponse.statusCode}`);
  const seed = JSON.parse(seedResponseBody.toString('utf8'));

  if (warmupSeconds > 0) {
    process.stdout.write(`Warming up for ${warmupSeconds}s...\n`);
    await sleep(warmupSeconds * 1000);
  }

  const create = await measure({
    baseUrl,
    durationMs: durationSeconds * 1000,
    concurrency: 10,
    expectedStatus: 201,
    request: async ({ baseUrl: requestBaseUrl, workerId, sequence }) => {
      const body = JSON.stringify({
        url: `https://example.com/performance/${workerId}/${sequence}`,
      });
      return requestHttp(`${requestBaseUrl}/api/v1/links`, {
        method: 'POST',
        headers: {
          'content-type': 'application/json',
          'content-length': String(Buffer.byteLength(body)),
          'x-forwarded-for': `198.51.100.${workerId + 1}`,
        },
        body,
      });
    },
  });

  const resolve = await measure({
    baseUrl,
    durationMs: durationSeconds * 1000,
    concurrency: 50,
    expectedStatus: 302,
    request: async () => requestHttp(seed.shortUrl),
  });

  const report = {
    warmupSeconds,
    durationSeconds,
    createConcurrency: 10,
    resolveConcurrency: 50,
    create,
    resolve,
  };
  process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);

  if (create.p95Ms === null || create.p95Ms > 100) {
    throw new Error(`create p95 target failed: ${create.p95Ms ?? 'n/a'}ms > 100ms`);
  }
  if (resolve.p95Ms === null || resolve.p95Ms > 50) {
    throw new Error(`resolve p95 target failed: ${resolve.p95Ms ?? 'n/a'}ms > 50ms`);
  }
  if (create.errorRate >= 0.001 || resolve.errorRate >= 0.001) {
    throw new Error('performance error-rate target failed: expected < 0.1%');
  }
} finally {
  httpAgent.destroy();
  await run(
    'docker',
    ['compose', ...composeArgs, 'down', '--volumes', '--remove-orphans'],
    env,
  ).catch(() => undefined);
}
