import { expect, test } from 'bun:test';
import { chapterTarget, compareReleases, pageKey, versionBase } from '../.vitepress/lib/versions.mjs';

const version = '0.57.0';
const index = {
  version,
  pages: {
    '': [],
    zh: [],
    learn: [],
    'learn/window.html': ['run-it'],
    'zh/learn/window.html': ['run-it'],
    'learn/styles.html': ['classes'],
  },
};

test('published releases sort numerically', () => {
  expect(['0.57.9', '0.57.10', '1.0.0'].sort(compareReleases)).toEqual(['0.57.9', '0.57.10', '1.0.0']);
  expect(() => versionBase('0.57-preview')).toThrow();
  expect(pageKey('/zh/index.html')).toBe('zh');
});

test('matching pages and anchors remain selected', () => {
  expect(chapterTarget(index, 'learn/window.html', 'en', '#run-it')).toBe('/docs/0.57.0/learn/window.html#run-it');
  expect(chapterTarget(index, 'learn/window.html', 'zh', '#run-it')).toBe('/docs/0.57.0/zh/learn/window.html#run-it');
});

test('version and language fallbacks stay within the requested version', () => {
  expect(chapterTarget(index, 'zh/learn/styles.html', 'zh', '#classes')).toBe('/docs/0.57.0/learn/styles.html?reading=language#classes');
  expect(chapterTarget(index, 'learn/styles', 'zh', '#classes')).toBe('/docs/0.57.0/learn/styles.html?reading=language#classes');
  expect(chapterTarget(index, 'learn/window.html', 'en', '#old-anchor')).toBe('/docs/0.57.0/learn/window.html?reading=anchor');
  expect(chapterTarget(index, 'learn/missing.html', 'en')).toBe('/docs/0.57.0/learn/?reading=page');
  expect(chapterTarget(index, 'zh/', 'en')).toBe('/docs/0.57.0/');
});

test('all 19 reference guides have Chinese counterparts with 1:1 anchor parity', async () => {
  const { existsSync, readFileSync } = await import('node:fs');
  const { resolve } = await import('node:path');
  const distIndex = resolve(import.meta.dir, '../.vitepress/dist/page-index.json');
  if (!existsSync(distIndex)) return;
  const built = JSON.parse(readFileSync(distIndex, 'utf8'));
  const guides = [
    'quickstart', 'state-management', 'composition', 'model-collections',
    'events', 'widget-events', 'navigation', 'scrolling',
    'typography', 'text-editing', 'clipboard', 'resources',
    'shared-textures', 'persistence', 'surface-sizing', 'backend-targets',
    'backend-fallback', 'timing-diagnostics', 'ci',
  ];
  for (const guide of guides) {
    const enIds = built.pages[`${guide}.html`];
    const zhIds = built.pages[`zh/${guide}.html`];
    expect(Array.isArray(enIds)).toBe(true);
    expect(Array.isArray(zhIds)).toBe(true);
    for (const id of enIds) {
      expect(zhIds).toContain(id);
      expect(chapterTarget(built, `${guide}.html`, 'zh', `#${id}`)).toBe(`/docs/${built.version}/zh/${guide}.html#${id}`);
      expect(chapterTarget(built, `zh/${guide}.html`, 'en', `#${id}`)).toBe(`/docs/${built.version}/${guide}.html#${id}`);
    }
  }
});
