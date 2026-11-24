import { execFileSync } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import { cp, mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { setTimeout } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';
import { isRelease } from '../.vitepress/lib/versions.mjs';
import { assembleSite, packageSnapshot, releaseNames, unpackSnapshot } from './snapshots.mjs';

const root = fileURLToPath(new URL('../..', import.meta.url));
const tutorial = join(root, 'docs/examples/first-app');
const repository = 'W-Mai/mirui';
const candidateName = 'mirui-docs-candidate';

function command(program, args, cwd = root) {
  return execFileSync(program, args, { cwd, stdio: ['ignore', 'pipe', 'inherit'], encoding: 'utf8' }).trim();
}

function gh(args) {
  return command('gh', [...args, '--repo', repository]);
}

async function downloadReleases(directory) {
  const pages = command('gh', ['api', '--paginate', '--slurp', `repos/${repository}/releases`]);
  const releases = [];
  for (const item of JSON.parse(pages).flat()) {
    const version = item.tag_name.replace(/^v/, '');
    if (item.draft || item.prerelease || item.tag_name !== `v${version}` || !isRelease(version)) continue;
    const { archive, marker } = releaseNames(version);
    if (!item.assets.some(asset => asset.name === marker)) continue;
    const target = join(directory, version);
    await mkdir(target);
    gh(['release', 'download', item.tag_name, '-D', target, '-p', marker, '-p', archive]);
    releases.push({ version, directory: target });
  }
  return releases;
}

async function verifyRegistry(version, directory) {
  const project = join(directory, 'docs/examples/first-app');
  await cp(tutorial, project, {
    recursive: true,
    filter: path => ['src', 'examples', 'tests', 'Cargo.toml', ''].includes(path.slice(tutorial.length + 1).split('/')[0]),
  });
  const font = 'src/gallery/demos/assets/ui_geist.mirx';
  await mkdir(join(directory, 'src/gallery/demos/assets'), { recursive: true });
  await cp(join(root, font), join(directory, font));
  const manifest = join(project, 'Cargo.toml');
  const text = await readFile(manifest, 'utf8');
  const workspaceMirui = /(^mirui = \{ )path = "\.\.\/\.\.\/\.\."/gm;
  if ([...text.matchAll(workspaceMirui)].length !== 2) throw new Error('Unexpected tutorial dependency declarations');
  await writeFile(manifest, text.replace(workspaceMirui, `$1version = "=${version}"`) + '\n[workspace]\n');
  await cp(join(root, 'rust-toolchain.toml'), join(directory, 'rust-toolchain.toml'));
  for (const args of [
    ['test'],
    ['check', '--locked', '--features', 'desktop', '--example', 'desktop'],
    ['check', '--locked', '--target', 'wasm32-unknown-unknown'],
  ]) {
    execFileSync('cargo', args, { cwd: project, stdio: 'inherit' });
  }
}

async function prepareRelease(version, source, directory) {
  const tag = `v${version}`;
  const names = releaseNames(version);
  const release = JSON.parse(gh(['release', 'view', tag, '--json', 'assets,isDraft,isPrerelease']));
  if (release.isDraft || release.isPrerelease) throw new Error(`A public release is required for ${tag}`);
  const assets = new Set(release.assets.map(asset => asset.name));
  if (assets.has(names.marker)) {
    gh(['release', 'download', tag, '-D', directory, '-p', names.marker, '-p', names.archive]);
    await unpackSnapshot(directory, join(directory, 'verified'), version, source);
    return;
  }

  const runs = JSON.parse(gh([
    'run', 'list', '--workflow', 'pages.yml', '--event', 'push', '--commit', source, '--branch', 'main',
    '--json', 'databaseId,conclusion',
  ]));
  const run = runs.find(item => item.conclusion === 'success');
  if (!run) throw new Error(`No successful Pages run for ${tag} at ${source}`);
  gh(['run', 'download', String(run.databaseId), '-n', candidateName, '-D', directory]);
  const marker = await unpackSnapshot(directory, join(directory, 'verified'), version, source);
  await verifyRegistry(version, join(directory, 'registry'));
  if (assets.has(names.archive)) {
    const previous = join(directory, 'previous');
    gh(['release', 'download', tag, '-D', previous, '-p', names.archive]);
    const digest = createHash('sha256').update(await readFile(join(previous, names.archive))).digest('hex');
    if (digest !== marker.sha256) throw new Error(`Existing archive for ${tag} differs from the verified candidate`);
  } else {
    gh(['release', 'upload', tag, join(directory, names.archive)]);
  }
  gh(['release', 'upload', tag, join(directory, names.marker)]);
}

async function deployRelease(version, source) {
  const key = `docs-${version}-${randomUUID()}`;
  const url = gh(['workflow', 'run', 'pages.yml', '--ref', 'main', '-f', `docs_request=${key}`]);
  let id = url.match(/\/actions\/runs\/(\d+)/)?.[1];
  for (let attempt = 0; !id && attempt < 24; attempt++) {
    const runs = JSON.parse(gh(['run', 'list', '--workflow', 'pages.yml', '--event', 'workflow_dispatch',
      '--json', 'displayTitle,databaseId']));
    id = runs.find(run => run.displayTitle === key)?.databaseId;
    if (!id) await setTimeout(5000);
  }
  if (!id) throw new Error(`Pages dispatch did not create a visible run for ${key}`);
  console.log(`Documentation deployment: https://github.com/${repository}/actions/runs/${id}`);
  execFileSync('gh', ['run', 'watch', String(id), '--exit-status', '--interval', '15', '--repo', repository],
    { cwd: root, stdio: 'inherit', timeout: 30 * 60 * 1000 });
  for (let attempt = 0; attempt < 24; attempt++) {
    const nonce = `?deployment=${id}&attempt=${attempt}`;
    try {
      const addresses = ['/docs/versions.json', `/docs/${version}/page-index.json`];
      const [manifest, index] = await Promise.all(addresses.map(async path => {
        const response = await fetch('https://mirui.rs' + path + nonce, { signal: AbortSignal.timeout(10000) });
        if (!response.ok) throw new Error(`HTTP ${response.status}`);
        return response.json();
      }));
      if (manifest.releases.includes(version) && index.version === version && index.source === source) return;
    } catch {
      // A successful deployment may take time to reach the public edge.
    }
    await setTimeout(5000);
  }
  throw new Error(`Pages completed but the public index does not yet confirm ${version} at ${source}`);
}

const [action, ...args] = process.argv.slice(2);
if (action === 'package' && args.length === 3) {
  const [directory, output, version] = args;
  console.log(await packageSnapshot(resolve(directory), resolve(output), version));
} else if (action === 'assemble' && args.length === 2) {
  const archives = await mkdtemp(join(tmpdir(), 'mirui-docs-archives-'));
  try {
    const releases = await downloadReleases(archives);
    console.log(await assembleSite(resolve(args[0]), resolve(args[1]), releases));
  } finally {
    await rm(archives, { recursive: true, force: true });
  }
} else if (action === 'promote' && args.length === 2) {
  const [version, source] = args;
  if (!isRelease(version) || !/^[a-f0-9]{40}$/.test(source)
      || command('git', ['rev-parse', 'HEAD']) !== source) {
    throw new Error('The release version and source must match the current release checkout');
  }
  const tag = `v${version}`;
  const remoteTag = command('git', ['ls-remote', '--tags', 'origin', `refs/tags/${tag}`, `refs/tags/${tag}^{}`]);
  const refs = new Map(remoteTag.split('\n').map(line => line.split('\t').reverse()));
  if ((refs.get(`refs/tags/${tag}^{}`) ?? refs.get(`refs/tags/${tag}`)) !== source) {
    throw new Error(`The remote tag for ${version} does not match ${source}`);
  }
  const directory = await mkdtemp(join(tmpdir(), 'mirui-docs-release-'));
  try {
    await prepareRelease(version, source, directory);
    await deployRelease(version, source);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
} else {
  throw new Error('Usage: site.mjs package <snapshot> <output> <version> | assemble <gallery> <dev> | promote <version> <commit>');
}
