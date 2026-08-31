import { createServer } from 'node:net';
import { spawn } from 'node:child_process';

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

function run(command, args, env, timeoutMs = 120_000) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { env, stdio: 'inherit' });
    let settled = false;
    let timedOut = false;
    let forceTimer;
    const timer = setTimeout(() => {
      timedOut = true;
      child.kill('SIGTERM');
      forceTimer = setTimeout(() => child.kill('SIGKILL'), 5_000);
    }, timeoutMs);
    const finish = (callback) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      clearTimeout(forceTimer);
      callback();
    };
    child.once('error', (error) =>
      finish(() =>
        reject(timedOut ? new Error(`${command} timed out after ${timeoutMs}ms`) : error),
      ),
    );
    child.once('exit', (code, signal) =>
      finish(() => {
        if (timedOut) reject(new Error(`${command} timed out after ${timeoutMs}ms`));
        else if (code === 0) resolve();
        else reject(new Error(`${command} exited with ${signal ?? code}`));
      }),
    );
  });
}

function runCapture(command, args, env, timeoutMs = 30_000) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { env, stdio: ['ignore', 'pipe', 'pipe'] });
    let stdout = '';
    let stderr = '';
    child.stdout.setEncoding('utf8');
    child.stderr.setEncoding('utf8');
    child.stdout.on('data', (chunk) => (stdout += chunk));
    child.stderr.on('data', (chunk) => (stderr += chunk));
    let settled = false;
    let timedOut = false;
    let forceTimer;
    const timer = setTimeout(() => {
      timedOut = true;
      child.kill('SIGTERM');
      forceTimer = setTimeout(() => child.kill('SIGKILL'), 5_000);
    }, timeoutMs);
    const finish = (callback) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      clearTimeout(forceTimer);
      callback();
    };
    child.once('error', (error) =>
      finish(() =>
        reject(timedOut ? new Error(`${command} timed out after ${timeoutMs}ms`) : error),
      ),
    );
    child.once('exit', (code, signal) =>
      finish(() => {
        if (timedOut) reject(new Error(`${command} timed out after ${timeoutMs}ms`));
        else if (code === 0) resolve(stdout);
        else reject(new Error(`${command} exited with ${signal ?? code}: ${stderr.slice(-1000)}`));
      }),
    );
  });
}

function fetchWithTimeout(url, options = {}) {
  return fetch(url, { signal: AbortSignal.timeout(10_000), ...options });
}

async function expectStaticResource(url, expectedContentTypes) {
  const response = await fetchWithTimeout(url, { redirect: 'manual' });
  const contentType = response.headers.get('content-type') ?? '';
  if (
    !response.ok ||
    !expectedContentTypes.some((expectedContentType) => contentType.startsWith(expectedContentType))
  ) {
    throw new Error(`static resource check failed for ${url}: ${response.status} ${contentType}`);
  }
  return response;
}

function assetContentTypes(assetPath) {
  if (assetPath.endsWith('.js')) return ['text/javascript', 'application/javascript'];
  if (assetPath.endsWith('.css')) return ['text/css'];
  throw new Error(`unsupported asset type in home page: ${assetPath}`);
}

const project = `myurl-smoke-${process.pid}`;
const port = await freePort();
const baseUrl = `http://127.0.0.1:${port}`;
const composeArgs = ['-f', 'docker-compose.yaml', '-p', project];
const env = {
  ...process.env,
  APP_PORT: String(port),
  PUBLIC_BASE_URL: baseUrl,
  NODE_ENV: 'development',
  IP_HASH_SECRET: 'compose-smoke-secret-that-is-at-least-32-bytes-long',
  REDIS_URL: 'redis://redis:6379/0',
  TURNSTILE_ENABLED: 'false',
  REDIS_PASSWORD: 'compose-smoke-password',
  REDIS_VOLUME_NAME: `${project}-redis-data`,
};

let mainError;
let teardownError;

try {
  await run('docker', ['compose', ...composeArgs, 'up', '-d', '--build', '--wait'], env);
  const live = await fetchWithTimeout(`${baseUrl}/health/live`);
  const ready = await fetchWithTimeout(`${baseUrl}/health/ready`);
  if (!live.ok || !ready.ok) throw new Error('health checks failed');

  const home = await expectStaticResource(`${baseUrl}/`, ['text/html']);
  const homeHtml = await home.text();
  const assetPaths = [...homeHtml.matchAll(/(?:src|href)="(\/assets\/[^"]+)"/g)].map(
    ([, path]) => path,
  );
  if (assetPaths.length === 0) throw new Error('home page does not reference built assets');
  await Promise.all(
    assetPaths.map((assetPath) =>
      expectStaticResource(`${baseUrl}${assetPath}`, assetContentTypes(assetPath)),
    ),
  );
  await expectStaticResource(`${baseUrl}/favicon.svg`, ['image/svg+xml']);
  await expectStaticResource(`${baseUrl}/robots.txt`, ['text/plain']);
  await expectStaticResource(`${baseUrl}/sitemap.xml`, ['application/xml', 'text/xml']);

  const create = await fetchWithTimeout(`${baseUrl}/api/links`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ url: 'https://example.com/compose-smoke' }),
  });
  if (create.status !== 201) throw new Error(`create failed: ${create.status}`);
  const created = await create.json();
  const redirect = await fetchWithTimeout(created.shortUrl, { redirect: 'manual' });
  if (
    redirect.status !== 302 ||
    redirect.headers.get('location') !== 'https://example.com/compose-smoke'
  ) {
    throw new Error('redirect smoke check failed');
  }

  await run('docker', ['compose', ...composeArgs, 'restart', 'redis'], env);
  await run('docker', ['compose', ...composeArgs, 'up', '-d', '--wait', 'redis'], env);
  let afterRedisRestart = await fetchWithTimeout(created.shortUrl, { redirect: 'manual' });
  if (afterRedisRestart.status !== 302) {
    afterRedisRestart = await fetchWithTimeout(created.shortUrl, { redirect: 'manual' });
  }
  if (afterRedisRestart.status !== 302) {
    throw new Error('app did not recover after the Redis-only restart');
  }

  await run('docker', ['compose', ...composeArgs, 'restart', 'app'], env);
  await run('docker', ['compose', ...composeArgs, 'up', '-d', '--wait', 'app'], env);
  const afterAppRestart = await fetchWithTimeout(created.shortUrl, { redirect: 'manual' });
  if (afterAppRestart.status !== 302) {
    throw new Error('app did not recover after the app restart');
  }

  const redisContainer = (
    await runCapture('docker', ['compose', ...composeArgs, 'ps', '-q', 'redis'], env)
  ).trim();
  const portBindings = (
    await runCapture(
      'docker',
      ['inspect', '--format', '{{json .HostConfig.PortBindings}}', redisContainer],
      env,
    )
  ).trim();
  if (portBindings !== '{}' && portBindings !== 'null') {
    throw new Error('Redis must not publish a host port');
  }

  const appContainer = (
    await runCapture('docker', ['compose', ...composeArgs, 'ps', '-q', 'app'], env)
  ).trim();
  const appBindings = JSON.parse(
    (
      await runCapture(
        'docker',
        ['inspect', '--format', '{{json .HostConfig.PortBindings}}', appContainer],
        env,
      )
    ).trim(),
  );
  const publishedAppBindings = appBindings?.['3000/tcp'] ?? [];
  if (publishedAppBindings.length !== 1 || publishedAppBindings[0].HostIp !== '127.0.0.1') {
    throw new Error('App must publish port 3000 only on loopback');
  }
} catch (error) {
  mainError = error;
}

try {
  await run('docker', ['compose', ...composeArgs, 'down', '--volumes', '--remove-orphans'], env);
} catch (error) {
  teardownError = error;
}

if (mainError && teardownError) {
  throw new AggregateError(
    [mainError, teardownError],
    'Compose smoke test and teardown both failed',
    { cause: mainError },
  );
}
if (mainError) throw mainError;
if (teardownError) throw new Error('Compose smoke teardown failed', { cause: teardownError });
