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

function runCapture(command, args, env) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { env, stdio: ['ignore', 'pipe', 'pipe'] });
    let stdout = '';
    let stderr = '';
    child.stdout.setEncoding('utf8');
    child.stderr.setEncoding('utf8');
    child.stdout.on('data', (chunk) => (stdout += chunk));
    child.stderr.on('data', (chunk) => (stderr += chunk));
    child.once('error', reject);
    child.once('exit', (code, signal) => {
      if (code === 0) resolve(stdout);
      else reject(new Error(`${command} exited with ${signal ?? code}: ${stderr.slice(-1000)}`));
    });
  });
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
};

let mainError;
let teardownError;

try {
  await run('docker', ['compose', ...composeArgs, 'up', '-d', '--build', '--wait'], env);
  const live = await fetch(`${baseUrl}/health/live`);
  const ready = await fetch(`${baseUrl}/health/ready`);
  if (!live.ok || !ready.ok) throw new Error('health checks failed');

  const create = await fetch(`${baseUrl}/api/links`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ url: 'https://example.com/compose-smoke' }),
  });
  if (create.status !== 201) throw new Error(`create failed: ${create.status}`);
  const created = await create.json();
  const redirect = await fetch(created.shortUrl, { redirect: 'manual' });
  if (
    redirect.status !== 302 ||
    redirect.headers.get('location') !== 'https://example.com/compose-smoke'
  ) {
    throw new Error('redirect smoke check failed');
  }

  await run('docker', ['compose', ...composeArgs, 'restart', 'redis', 'app'], env);
  await run('docker', ['compose', ...composeArgs, 'up', '-d', '--wait'], env);
  const afterRestart = await fetch(created.shortUrl, { redirect: 'manual' });
  if (afterRestart.status !== 302) throw new Error('persistent redirect check failed');

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
