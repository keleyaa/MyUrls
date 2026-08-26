import { readFile, readdir } from 'node:fs/promises';
import path from 'node:path';

const root = path.resolve('apps/web/dist');
const allowedHosts = new Set([
  'challenges.cloudflare.com',
  'github.com',
  'sub.ml1.one',
  'example.com',
]);
const generatedDocumentationHosts = new Set(['svelte.dev', 'www.w3.org']);

async function filesIn(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const fullPath = path.join(directory, entry.name);
    if (entry.isDirectory()) files.push(...(await filesIn(fullPath)));
    else files.push(fullPath);
  }
  return files;
}

const files = await filesIn(root);
if (files.length === 0) throw new Error('web build is empty');
const disallowed = [];
for (const file of files) {
  if (!/\.(?:html|js|css)$/i.test(file)) continue;
  const source = await readFile(file, 'utf8');
  for (const match of source.matchAll(/https?:\/\/([^/"'\s)]+)/gi)) {
    const host = (match[1] ?? '').toLowerCase().replace(/:\d+$/, '');
    if (generatedDocumentationHosts.has(host)) continue;
    if (!allowedHosts.has(host) && !host.endsWith('.cloudflare.com')) {
      disallowed.push(`${path.relative(process.cwd(), file)} -> ${host}`);
    }
  }
  if (/fonts\.googleapis|unpkg\.com|jsdelivr\.net|cdnjs\.cloudflare\.com/i.test(source)) {
    disallowed.push(`${path.relative(process.cwd(), file)} -> runtime CDN`);
  }
}
if (disallowed.length > 0) {
  throw new Error(`runtime external resource scan failed:\n${disallowed.join('\n')}`);
}
