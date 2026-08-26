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

function run(command, args, env = process.env) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { env, stdio: 'inherit' });
    child.once('error', reject);
    child.once('exit', (code, signal) => {
      if (code === 0) {
        resolve();
      } else {
        reject(new Error(`${command} exited with ${signal ?? code}`));
      }
    });
  });
}

const project = `myurl-v2-integration-${process.pid}`;
const port = await freePort();
const composeArgs = [
  '-f',
  'docker-compose.yaml',
  '-f',
  'ops/docker-compose.verify.yaml',
  '-p',
  project,
];
const env = { ...process.env, REDIS_VERIFY_PORT: String(port) };

try {
  await run('docker', ['compose', ...composeArgs, 'up', '-d', '--wait', 'redis'], env);
  await run(
    'corepack',
    ['pnpm', 'exec', 'vitest', 'run', '--config', 'vitest.integration.config.ts'],
    {
      ...env,
      REDIS_URL: `redis://127.0.0.1:${port}/15`,
    },
  );
} finally {
  await run(
    'docker',
    ['compose', ...composeArgs, 'down', '--volumes', '--remove-orphans'],
    env,
  ).catch(() => undefined);
}
