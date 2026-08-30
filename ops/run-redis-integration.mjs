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
const childEnv = { ...process.env, REDIS_VERIFY_PORT: String(port) };
delete childEnv.REDIS_PASSWORD;
delete childEnv.MYURL_REDIS_INTEGRATION;

let mainError;
let teardownError;

try {
  await run('docker', ['compose', ...composeArgs, 'up', '-d', '--wait', 'redis'], childEnv);
  await run(
    'cargo',
    ['test', '-p', 'myurl-server', '--all-features', '--test', 'redis', '--', '--ignored'],
    {
      ...childEnv,
      MYURL_REDIS_INTEGRATION: '1',
      REDIS_URL: `redis://127.0.0.1:${port}/15`,
    },
  );
} catch (error) {
  mainError = error;
}

try {
  await run(
    'docker',
    ['compose', ...composeArgs, 'down', '--volumes', '--remove-orphans'],
    childEnv,
  );
} catch (error) {
  teardownError = error;
}

if (mainError && teardownError) {
  throw new AggregateError(
    [mainError, teardownError],
    'Integration test and Docker Compose teardown both failed',
    { cause: mainError },
  );
}
if (mainError) throw mainError;
if (teardownError) throw new Error('Docker Compose teardown failed', { cause: teardownError });
