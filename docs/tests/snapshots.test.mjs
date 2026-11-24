import { cp, mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { expect, test } from 'bun:test';
import { assembleSite, packageSnapshot, releaseNames, unpackSnapshot } from '../scripts/snapshots.mjs';

const source = 'a'.repeat(40);
const pages = Object.fromEntries(['', 'zh', 'learn', 'zh/learn', 'learn/motion.html', 'zh/learn/motion.html']
  .map(key => [key, ['run']]));

async function fixture(root, version) {
  const path = join(root, version);
  await mkdir(join(path, 'play'), { recursive: true });
  await writeFile(join(path, 'page-index.json'), JSON.stringify({ version, source, crateVersion: version, pages }));
  await writeFile(join(path, 'index.html'), `<script src="/docs/${version}/assets/app.js"></script>`);
  await writeFile(join(path, 'play/mirui-first-app_bg.wasm'), Uint8Array.of(0, 97, 115, 109, 1, 0, 0, 0));
  await writeFile(join(path, 'sitemap.xml'),
    `<urlset><url><loc>https://mirui.rs/docs/${version}/</loc></url></urlset>`);
  return path;
}

async function gallery(root, name) {
  const path = join(root, name);
  await mkdir(join(path, 'chord-prompter'), { recursive: true });
  await writeFile(join(path, 'index.html'), '<a href="/docs/">Docs</a>');
  await writeFile(join(path, 'chord-prompter/index.html'), 'Chord prompter');
  await writeFile(join(path, 'sitemap.xml'), '<urlset><url><loc>https://mirui.rs/</loc></url></urlset>');
  return path;
}

test('release identity, archive hash and valid Wasm are required', async () => {
  const root = await mkdtemp(join(tmpdir(), 'docs-snapshot-test-'));
  try {
    const version = '0.57.0';
    const input = await fixture(root, version);
    const output = join(root, 'archive');
    const target = join(root, 'unpacked');
    const marker = await packageSnapshot(input, output, version);
    expect(marker.source).toBe(source);
    expect(await unpackSnapshot(output, target, version, source)).toEqual(marker);
    await expect(unpackSnapshot(output, target, version, 'b'.repeat(40))).rejects.toThrow('Invalid release marker');
    await writeFile(join(output, releaseNames(version).archive), 'incomplete');
    await expect(unpackSnapshot(output, target, version, source)).rejects.toThrow('Invalid archive digest');
    await writeFile(join(input, 'play/mirui-first-app_bg.wasm'), 'incomplete');
    await expect(packageSnapshot(input, output, version)).rejects.toThrow();
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('assembly preserves prior releases and ignores an unpromoted candidate', async () => {
  const root = await mkdtemp(join(tmpdir(), 'docs-assembly-test-'));
  try {
    const dev = await fixture(root, 'dev');
    const site = await gallery(root, 'site');
    const old = '0.9.0';
    const next = '0.10.0';
    const oldArchive = join(root, 'old-archive');
    const nextArchive = join(root, 'next-archive');
    await packageSnapshot(await fixture(root, old), oldArchive, old);
    await packageSnapshot(await fixture(root, next), nextArchive, next);
    expect((await assembleSite(site, dev, [])).latest).toBeNull();
    expect(await readFile(join(site, 'docs/index.html'), 'utf8')).toContain('href="/docs/dev/"');

    const oldSite = await gallery(root, 'old-site');
    expect((await assembleSite(oldSite, dev, [{ version: old, directory: oldArchive }])).latest).toBe(old);
    const pendingSite = await gallery(root, 'pending-site');
    const pending = join(root, 'incomplete-archive');
    await cp(nextArchive, pending, { recursive: true });
    await rm(join(pending, releaseNames(next).marker));
    await expect(assembleSite(pendingSite, dev, [{ version: next, directory: pending }])).rejects.toThrow();
    await expect(readFile(join(pendingSite, 'docs/versions.json'))).rejects.toThrow();

    const newSite = await gallery(root, 'new-site');
    const releases = [{ version: old, directory: oldArchive }, { version: next, directory: nextArchive }];
    expect((await assembleSite(newSite, dev, releases)).releases).toEqual([next, old]);
    expect(await readFile(join(newSite, 'docs/index.html'), 'utf8')).toContain('url=/docs/0.10.0/');
    expect(await readFile(join(newSite, 'docs', old, 'index.html'), 'utf8'))
      .toBe(await readFile(join(root, old, 'index.html'), 'utf8'));
    expect(await readFile(join(newSite, 'chord-prompter/index.html'), 'utf8')).toBe('Chord prompter');
    expect(await readFile(join(newSite, 'sitemap.xml'), 'utf8')).toContain('https://mirui.rs/docs/0.10.0/');
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
