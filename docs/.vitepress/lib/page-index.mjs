import { readdir, readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { parse } from 'parse5';
import { pageKey, versionBase } from './versions.mjs';

export async function indexPages(directory, context) {
  const pages = {};
  const links = [];
  const base = versionBase(context.version);
  const origin = 'https://mirui.rs';
  const files = (await readdir(directory, { recursive: true })).sort();
  for (const file of files) {
    if (!file.endsWith('.html') || file === '404.html' || file.startsWith('play/')) continue;
    const ids = [];
    function visit(node) {
      const id = node.attrs?.find(attr => attr.name === 'id')?.value;
      if (id) ids.push(id);
      if (node.nodeName === 'a') {
        const href = node.attrs.find(attr => attr.name === 'href')?.value;
        if (href && href !== '#') links.push({ file, href });
      }
      for (const child of node.childNodes ?? []) visit(child);
    }
    visit(parse(await readFile(join(directory, file), 'utf8')));
    pages[pageKey(file)] = ids;
  }
  const broken = [];
  for (const { file, href } of links) {
    const target = new URL(href, origin + base + file);
    if (target.origin !== origin || !target.pathname.startsWith(base)) continue;
    const path = pageKey(decodeURI(target.pathname.slice(base.length)));
    const ids = pages[path] ?? pages[path + '.html'];
    const anchor = decodeURIComponent(target.hash.slice(1));
    if ((!ids && !files.includes(path)) || (ids && anchor && !ids.includes(anchor))) {
      broken.push(`${file}: ${href}`);
    }
  }
  if (broken.length) throw new Error('Broken documentation links:\n' + [...new Set(broken)].join('\n'));
  await writeFile(join(directory, 'page-index.json'), JSON.stringify({ ...context, pages }) + '\n');
}
