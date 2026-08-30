import { execFile } from 'node:child_process';

function commandExists(command) {
  return new Promise((resolve) => {
    execFile('sh', ['-c', `command -v ${command}`], (error) => resolve(error === null));
  });
}

function run(command, args) {
  return new Promise((resolve, reject) => {
    execFile(command, args, { encoding: 'utf8' }, (error, stdout, stderr) => {
      if (error) reject(new Error(`${command} failed: ${stderr || stdout || error.message}`));
      else resolve(stdout);
    });
  });
}

await run('docker', ['compose', 'build', '--quiet', 'app']);
const imageId = 'myurl:local';

if (await commandExists('trivy')) {
  await run('trivy', [
    'image',
    '--scanners',
    'vuln',
    '--exit-code',
    '1',
    '--severity',
    'HIGH,CRITICAL',
    '--ignore-unfixed',
    imageId,
  ]);
} else {
  throw new Error(
    'local container vulnerability scanner unavailable; install Trivy. Docker Scout is disabled because it may send image SBOM metadata to an external service',
  );
}
