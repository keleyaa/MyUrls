import { createHash } from 'node:crypto';
import { execFile, spawn } from 'node:child_process';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';

const image =
  'redis:8.10.0@sha256:c29e49ab2f85760a3827b53882e6dd9f5c6c3f0bb7d724e07bb31cbf275a5236';
const stamp = `${process.pid}-${Date.now()}`;
const sourceContainer = `myurl-v2-backup-source-${stamp}`;
const restoredContainer = `myurl-v2-backup-restored-${stamp}`;
const sourceVolume = `myurl-v2-backup-source-${stamp}`;
const restoredVolume = `myurl-v2-backup-restored-${stamp}`;
const workDir = await mkdtemp(path.join(tmpdir(), 'myurl-v2-backup-'));
const backupFile = path.join(workDir, 'redis-test.rdb');

function exec(command, args, options = {}) {
  return new Promise((resolve, reject) => {
    execFile(
      command,
      args,
      { ...options, encoding: options.encoding ?? 'utf8', maxBuffer: 16 * 1024 * 1024 },
      (error, stdout, stderr) => {
        if (error) reject(new Error(`${command} failed: ${stderr || error.message}`));
        else resolve(stdout);
      },
    );
  });
}

function run(command, args) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { stdio: 'inherit' });
    child.once('error', reject);
    child.once('exit', (code, signal) => {
      if (code === 0) resolve();
      else reject(new Error(`${command} exited with ${signal ?? code}`));
    });
  });
}

async function waitForRedis(container) {
  for (let attempt = 0; attempt < 30; attempt += 1) {
    try {
      const pong = await exec('docker', ['exec', container, 'redis-cli', 'ping']);
      if (String(pong).trim() === 'PONG') return;
    } catch {
      await new Promise((resolve) => setTimeout(resolve, 500));
    }
  }
  throw new Error('Redis did not become ready');
}

try {
  await exec('docker', ['volume', 'create', sourceVolume]);
  await run('docker', [
    'run',
    '-d',
    '--name',
    sourceContainer,
    '--mount',
    `type=volume,source=${sourceVolume},target=/data`,
    image,
    'redis-server',
    '--appendonly',
    'yes',
    '--appendfsync',
    'everysec',
  ]);
  await waitForRedis(sourceContainer);
  await exec('docker', [
    'exec',
    sourceContainer,
    'redis-cli',
    'SET',
    'myurl:link:backup-test',
    'https://example.com/backup',
    'EX',
    '7776000',
  ]);
  const rdb = await exec('docker', ['exec', sourceContainer, 'redis-cli', '--rdb', '-'], {
    encoding: 'buffer',
  });
  await writeFile(backupFile, rdb);
  const checksum = createHash('sha256').update(rdb).digest('hex');
  await writeFile(`${backupFile}.sha256`, `${checksum}  ${path.basename(backupFile)}\n`);

  await run('sh', ['ops/redis-restore.sh', backupFile, restoredVolume, image]);
  await run('docker', [
    'run',
    '-d',
    '--name',
    restoredContainer,
    '--mount',
    `type=volume,source=${restoredVolume},target=/data`,
    image,
    'redis-server',
    '--appendonly',
    'yes',
    '--appendfsync',
    'everysec',
  ]);
  await waitForRedis(restoredContainer);
  const value = await exec('docker', [
    'exec',
    restoredContainer,
    'redis-cli',
    'GET',
    'myurl:link:backup-test',
  ]);
  if (String(value).trim() !== 'https://example.com/backup')
    throw new Error('restored value mismatch');
} finally {
  await exec('docker', ['rm', '-f', sourceContainer, restoredContainer]).catch(() => undefined);
  await exec('docker', ['volume', 'rm', sourceVolume, restoredVolume]).catch(() => undefined);
  await rm(workDir, { recursive: true, force: true });
}
