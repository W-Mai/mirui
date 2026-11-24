import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { cp, mkdir, mkdtemp, readFile, readdir, rename, rm, stat, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { compareReleases, isRelease, versionBase } from '../.vitepress/lib/versions.mjs';

const origin = 'https://mirui.rs';
const shaPattern = /^[a-f0-9]{40}$/;
const hashPattern = /^[a-f0-9]{64}$/;

export function releaseNames(version) {
  if (!isRelease(version)) throw new Error(`Invalid release version: ${version}`);
  return { archive: `mirui-docs-${version}.tar.gz`, marker: `mirui-docs-${version}.json` };
}

async function fileHash(file) {
  return createHash('sha256').update(await readFile(file)).digest('hex');
}

async function readJson(file) {
  return JSON.parse(await readFile(file, 'utf8'));
}

export async function checkSnapshot(directory, version, source) {
  const index = await readJson(join(directory, 'page-index.json'));
  if (index.version !== version || !shaPattern.test(index.source) || (source && index.source !== source)) {
    throw new Error(`Unexpected identity for documentation snapshot ${directory}`);
  }
  if (isRelease(version) && index.crateVersion !== version) {
    throw new Error(`Documentation and crate versions differ for ${version}`);
  }
  for (const key of ['', 'zh', 'learn', 'zh/learn', 'learn/motion.html', 'zh/learn/motion.html']) {
    if (!Object.hasOwn(index.pages, key)) throw new Error(`Snapshot ${version} is missing ${key}`);
  }
  const home = await readFile(join(directory, 'index.html'), 'utf8');
  if (!home.includes(versionBase(version) + 'assets/')) {
    throw new Error(`Snapshot ${version} uses the wrong base URL`);
  }
  await WebAssembly.compile(await readFile(join(directory, 'play/mirui-first-app_bg.wasm')));
  return index;
}

export async function packageSnapshot(directory, output, version) {
  const index = await checkSnapshot(directory, version);
  const names = releaseNames(version);
  await mkdir(output, { recursive: true });
  const archive = resolve(output, names.archive);
  execFileSync('tar', ['-czf', archive, '-C', directory, '.']);
  const marker = { version, source: index.source, sha256: await fileHash(archive) };
  await writeFile(join(output, names.marker), JSON.stringify(marker) + '\n');
  return marker;
}

export async function unpackSnapshot(input, directory, version, source) {
  const names = releaseNames(version);
  const marker = await readJson(join(input, names.marker));
  if (marker.version !== version || !shaPattern.test(marker.source) || (source && marker.source !== source)
      || !hashPattern.test(marker.sha256)) {
    throw new Error(`Invalid release marker for ${version}`);
  }
  const archive = join(input, names.archive);
  if (await fileHash(archive) !== marker.sha256) throw new Error(`Invalid archive digest for ${version}`);
  const entries = execFileSync('tar', ['-tvzf', archive], { encoding: 'utf8' }).trim().split('\n');
  for (const entry of entries) {
    const file = entry.slice(entry.indexOf(' ./') + 1).trim();
    if ((!entry.startsWith('-') && !entry.startsWith('d')) || !file.startsWith('./') || file.split('/').includes('..')) {
      throw new Error(`Unexpected entry in documentation archive: ${entry}`);
    }
  }
  await mkdir(directory, { recursive: true });
  execFileSync('tar', ['-xzf', archive, '-C', directory]);
  await checkSnapshot(directory, version, marker.source);
  return marker;
}

function indexHtml(latest) {
  const target = versionBase(latest ?? 'dev');
  const refresh = latest ? `<meta http-equiv="refresh" content="0; url=${target}">` : '';
  const title = latest ? 'Mirui documentation' : 'Mirui development documentation';
  return `<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">${refresh}<title>${title}</title><link rel="canonical" href="${origin}${target}"></head>
<body><main><h1>${title}</h1><p><a href="${target}">Open the manual</a></p><p><a href="${target}zh/">简体中文</a></p>${latest ? '<p><a href="/docs/dev/">Development documentation</a></p>' : ''}<p><a href="https://docs.rs/mirui">Published API reference</a></p></main></body></html>\n`;
}

async function treeBytes(path) {
  const info = await stat(path);
  if (!info.isDirectory()) return info.size;
  let total = 0;
  for (const file of await readdir(path)) total += await treeBytes(join(path, file));
  return total;
}

export async function assembleSite(site, dev, releases) {
  await checkSnapshot(dev, 'dev');
  const versions = releases.map(({ version }) => version).sort(compareReleases).reverse();
  if (new Set(versions).size !== versions.length) throw new Error('Duplicate documentation versions');
  const snapshot = await mkdtemp(join(tmpdir(), 'mirui-docs-'));
  try {
    for (const item of releases) {
      await unpackSnapshot(item.directory, join(snapshot, item.version), item.version);
    }
    await cp(dev, join(snapshot, 'dev'), { recursive: true });
    const latest = versions[0] ?? null;
    const manifest = { latest, releases: versions, development: 'dev' };
    await writeFile(join(snapshot, 'versions.json'), JSON.stringify(manifest) + '\n');
    await writeFile(join(snapshot, 'index.html'), indexHtml(latest));
    const baseSitemap = join(site, 'sitemap.xml');
    const sitemap = await readFile(baseSitemap, 'utf8');
    const marked = sitemap.indexOf('</urlset>');
    if (marked < 0) throw new Error('Gallery sitemap is not a URL set');
    const docsSitemap = await readFile(join(snapshot, latest ?? 'dev', 'sitemap.xml'), 'utf8');
    const urls = docsSitemap.match(/<url>[\s\S]*?<\/url>/g) ?? [];
    const bytes = await treeBytes(site) + await treeBytes(snapshot);
    if (bytes >= 1_000_000_000) throw new Error(`Pages exceeds its 1 GB limit (${bytes} bytes)`);
    await cp(snapshot, join(site, 'docs'), { recursive: true, errorOnExist: true, force: false });
    await writeFile(baseSitemap + '.new', sitemap.slice(0, marked) + urls.join('\n') + '\n</urlset>\n');
    await rename(baseSitemap + '.new', baseSitemap);
    return { ...manifest, bytes };
  } finally {
    await rm(snapshot, { recursive: true, force: true });
  }
}
