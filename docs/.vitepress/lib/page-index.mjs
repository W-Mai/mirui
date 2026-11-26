import { readdir, readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { parse } from 'parse5';
import { pageKey, versionBase } from './versions.mjs';

function buildLlmsTxt(base, context) {
  const origin = 'https://mirui.rs' + base;
  return `# Mirui (${context.version === 'dev' ? 'development' : 'v' + context.version}, crate ${context.crateVersion})

> Possibly the most elegant declarative embedded graphics library in Rust.
> \`no_std\` + \`alloc\`, ECS-driven scene graph, reactive \`#[model]\` bindings, spring/tween motion, shaped bidirectional & path typography, and pluggable backends (software framebuffer, SDL2 CPU/GPU, WGPU, Web Canvas, Linux DRM/fbdev, NuttX).

- Repository: https://github.com/W-Mai/mirui (commit \`${context.source}\`)
- Live Gallery: https://mirui.rs/
- API Reference: https://docs.rs/mirui

## Authoring Cheat Sheet (Macros & Contracts)

- **\`#[model]\` / \`#[observe]\` / \`#[persist]\`**:
  Declare reactive application state on structs and \`impl\` blocks. Observed fields generate getter methods (\`task.done()\`). Always call the getter inside \`\${...}\` so the reactive scope records the dependency. Model methods return owned values.
- **\`#[compose]\` & \`ui! { ... }\`**:
  Prepare stack-held UI compositions. Pass reactive inputs with \`prop!(T)\` and models with \`model!(T)\`. Key dynamic lists with \`walk \${ list.entries() } with row by row.0 { ... }\`.
- **\`#[event]\`**:
  Define typed event callbacks (\`on Tap(my_event::callback(arg, model))\`). Parameters can request \`event!(Tap)\`, \`entity!()\`, \`model!(T)\`, \`res!(T)\`, \`query!(...)\`, and \`commands!()\`.
- **\`#[animate(component = ...)]\` & \`FollowBounds\`**:
  Drive smooth transitions from ECS systems or attach \`FollowBounds::new(id("target")).with_transition(INTERACTIVE.into())\` to let a view chase another widget's layout bounds.
- **\`#[component]\` & \`#[view(component = ...)]\`**:
  Extend \`ui!\` with custom widgets that emit \`DrawCommand::Fill\`, \`DrawCommand::FillPath\`, or \`DrawCommand::StrokePath\` against \`paint.rect()\`.

## Tutorial (7 Runnable Checkpoints)

- [Overview (EN)](${origin}learn/): Build a complete task list from a bare window to WebAssembly.
- [亲手写一个任务清单 (ZH)](${origin}zh/learn/): 七章互动课程。
- [01 · Open a window](${origin}learn/window.html) / [打开窗口](${origin}zh/learn/window.html)
- [02 · Complete a task](${origin}learn/state.html) / [点亮第一枚勾选框](${origin}zh/learn/state.html)
- [03 · Add, remove, and filter](${origin}learn/collections.html) / [增删与筛选列表](${origin}zh/learn/collections.html)
- [04 · Reuse components and styles](${origin}learn/components.html) / [提炼组件与样式表](${origin}zh/learn/components.html)
- [05 · Give the interface motion](${origin}learn/motion.html) / [注入物理动效](${origin}zh/learn/motion.html)
- [06 · Save and restore the list](${origin}learn/storage.html) / [断电与重开之后](${origin}zh/learn/storage.html)
- [07 · Run it in the browser](${origin}learn/web.html) / [编译进浏览器画布](${origin}zh/learn/web.html)

## Common Patterns & Interface Guides

- [Common Patterns (EN)](${origin}learn/recipes.html) / [常用界面写法 (ZH)](${origin}zh/learn/recipes.html): Live examples for reactive thresholds, flex sizing, spring FollowBounds, and custom vector views.
- [Quickstart & Targets](${origin}quickstart.html): Toolchain setup for desktop SDL, ESP32-C3, Android, iOS, and multi-target workspaces.
- [Reactive State & Styles](${origin}state-management.html): Models, \`Signal<T>\`, \`Computed<T>\`, \`StyleSheetPlugin\`, and flush boundaries.
- [Composition, Roles & Views](${origin}composition.html): \`#[compose]\`, \`prop!(T)\`, \`res!(T)\`, \`#[system]\`, \`#[view]\`, and \`ui!(compose Name)\` niches.
- [Model Collections](${origin}model-collections.html): Bounded \`ModelList\`, \`ModelMap\`, \`ModelSet\`, and stable \`ModelItemId\` keys.
- [Events & Callbacks](${origin}events.html): Gesture recognition, bubbling, and \`#[event]\` handlers.
- [Navigation & Motion](${origin}navigation.html): \`FollowBounds\`, \`NavigationStack\`, \`PageView\`, springs, and tweens.
- [Scrolling](${origin}scrolling.html): \`ScrollOffset\`, \`ScrollConfig\`, momentum, and nested scrollers.

## Views, Typography, Assets & Backends

- [Building Custom Widgets](${origin}widget-events.html): Custom \`#[view]\` descriptors, input delivery, and \`TextDocument\` / \`TextEditing\` internals.
- [Typography](${origin}typography.html): Shaped text, \`FontToken::FALLBACK\`, bidirectional scripts, and text along Bézier paths (\`TextPath\`).
- [Text Editing](${origin}text-editing.html): Custom text editors, selection geometry, and IME commit handling.
- [Clipboard](${origin}clipboard.html): Cross-platform clipboard integration.
- [Resources](${origin}resources.html): \`ResourceManager\`, static/factory entries, and bounded caches.
- [Shared Textures](${origin}shared-textures.html): Cross-thread \`sync\` textures and device upload lifetimes.
- [Persistence](${origin}persistence.html): \`PersistencePlugin\`, \`#[persist]\`, and storage backends.
- [Surface Sizing](${origin}surface-sizing.html): Logical viewports, HiDPI scale factors, and dynamic resizing.
- [Backend Targets](${origin}backend-targets.html): Software rasterizer, SDL2, SDL-GPU, WGPU, Web Canvas, Linux DRM/fbdev, and NuttX.
- [Backend Fallback](${origin}backend-fallback.html): \`compose_backend!\`, partial damage redraws, and offscreen composition.
- [Timing Diagnostics](${origin}timing-diagnostics.html): Frame-phase profiling with the \`perf\` feature.
- [CI Checks](${origin}ci.html): Compile-fail and architectural contract tests.
`;
}

export async function indexPages(directory, context) {
  const pages = {};
  const links = [];
  const base = versionBase(context.version);
  const origin = 'https://mirui.rs';
  await writeFile(join(directory, 'llms.txt'), buildLlmsTxt(base, context));
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
