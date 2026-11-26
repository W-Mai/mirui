import libRaw from '../../examples/first-app/src/lib.rs?raw';
import modelRaw from '../../examples/first-app/src/model.rs?raw';
import interfaceRaw from '../../examples/first-app/src/interface.rs?raw';
import motionRaw from '../../examples/first-app/src/motion.rs?raw';
import webRaw from '../../examples/first-app/src/web.rs?raw';
import desktopRaw from '../../examples/first-app/examples/desktop.rs?raw';
import recipesRaw from '../../examples/first-app/src/recipes.rs?raw';

const RUST_KEYWORDS = new Set([
  'as', 'async', 'await', 'break', 'const', 'continue', 'crate', 'dyn',
  'else', 'enum', 'extern', 'false', 'fn', 'for', 'if', 'impl', 'in',
  'let', 'loop', 'match', 'mod', 'move', 'mut', 'pub', 'ref', 'return',
  'self', 'Self', 'static', 'struct', 'super', 'trait', 'true', 'type',
  'union', 'unsafe', 'use', 'where', 'while',
]);

export function escapeHtml(s) {
  return s.replace(/[&<>]/g, c => (c === '&' ? '&amp;' : c === '<' ? '&lt;' : '&gt;'));
}

export function highlightRust(src) {
  let out = '';
  let i = 0;
  const n = src.length;
  const isIdentStart = c => /[A-Za-z_]/.test(c);
  const isIdent = c => /[A-Za-z0-9_]/.test(c);

  while (i < n) {
    const c = src[i];
    const c2 = src[i + 1];

    if (c === '/' && c2 === '/') {
      let j = i + 2;
      while (j < n && src[j] !== '\n') j++;
      out += `<span class="tok-com">${escapeHtml(src.slice(i, j))}</span>`;
      i = j;
      continue;
    }
    if (c === '/' && c2 === '*') {
      let j = i + 2;
      let depth = 1;
      while (j < n && depth > 0) {
        if (src[j] === '/' && src[j + 1] === '*') { depth++; j += 2; }
        else if (src[j] === '*' && src[j + 1] === '/') { depth--; j += 2; }
        else j++;
      }
      out += `<span class="tok-com">${escapeHtml(src.slice(i, j))}</span>`;
      i = j;
      continue;
    }
    if (c === '"') {
      let j = i + 1;
      while (j < n) {
        if (src[j] === '\\') { j += 2; continue; }
        if (src[j] === '"') { j++; break; }
        j++;
      }
      out += `<span class="tok-str">${escapeHtml(src.slice(i, j))}</span>`;
      i = j;
      continue;
    }
    if (c === "'") {
      let j = i + 1;
      if (src[j] === '\\') j += 2;
      else j += 1;
      if (src[j] === "'") {
        out += `<span class="tok-str">${escapeHtml(src.slice(i, j + 1))}</span>`;
        i = j + 1;
      } else {
        out += escapeHtml(c);
        i += 1;
      }
      continue;
    }
    if (c === '#' && (c2 === '[' || c2 === '!')) {
      let j = i + 1;
      if (src[j] === '!') j++;
      if (src[j] === '[') {
        let depth = 0;
        while (j < n) {
          if (src[j] === '[') depth++;
          else if (src[j] === ']') { depth--; j++; if (depth === 0) break; continue; }
          j++;
        }
        out += `<span class="tok-attr">${escapeHtml(src.slice(i, j))}</span>`;
        i = j;
        continue;
      }
    }
    if (/[0-9]/.test(c)) {
      let j = i + 1;
      while (j < n && /[0-9a-fA-FxXoObB_.eE]/.test(src[j])) j++;
      out += `<span class="tok-num">${escapeHtml(src.slice(i, j))}</span>`;
      i = j;
      continue;
    }
    if (isIdentStart(c)) {
      let j = i + 1;
      while (j < n && isIdent(src[j])) j++;
      const word = src.slice(i, j);
      if (src[j] === '!') {
        out += `<span class="tok-mac">${escapeHtml(word)}!</span>`;
        i = j + 1;
      } else if (RUST_KEYWORDS.has(word)) {
        out += `<span class="tok-kw">${escapeHtml(word)}</span>`;
        i = j;
      } else if (/^[A-Z]/.test(word)) {
        out += `<span class="tok-type">${escapeHtml(word)}</span>`;
        i = j;
      } else {
        out += escapeHtml(word);
        i = j;
      }
      continue;
    }
    out += escapeHtml(c);
    i += 1;
  }
  return out;
}

export function highlightLines(text) {
  const lines = [];
  let line = '';
  let open = '';
  for (const piece of highlightRust(text).split(/(<span class="[^"]+">|<\/span>|\n)/)) {
    if (piece.startsWith('<span')) { open = piece; line += piece; }
    else if (piece === '</span>') { open = ''; line += piece; }
    else if (piece === '\n') { lines.push(open ? `${line}</span>` : line); line = open; }
    else line += piece;
  }
  lines.push(line);
  return lines;
}

function parseAnnotatedFile(name, path, raw) {
  const rawLines = raw.replace(/\r\n/g, '\n').split('\n');
  const lines = [];
  const focusByKey = new Map();
  const addByKey = new Map();
  const activeFocus = new Set();
  const activeAdd = new Set();

  const parseTags = (line, prefix) => {
    const trimmed = line.trim();
    if (!trimmed.startsWith(prefix)) return null;
    const rest = trimmed.slice(prefix.length).replace(/^:/, '').trim();
    return rest ? rest.split(',').map(s => s.trim()).filter(Boolean) : ['*'];
  };

  for (const rawLine of rawLines) {
    const trimmed = rawLine.trim();
    if (trimmed.startsWith('// #region') || trimmed.startsWith('// #endregion')) {
      continue;
    }
    const fStart = parseTags(trimmed, '//~focus-start');
    if (fStart) { fStart.forEach(k => activeFocus.add(k)); continue; }
    const fEnd = parseTags(trimmed, '//~focus-end');
    if (fEnd) { fEnd.forEach(k => activeFocus.delete(k)); continue; }
    const aStart = parseTags(trimmed, '//~add-start');
    if (aStart) { aStart.forEach(k => activeAdd.add(k)); continue; }
    const aEnd = parseTags(trimmed, '//~add-end');
    if (aEnd) { aEnd.forEach(k => activeAdd.delete(k)); continue; }

    lines.push(rawLine);
    const lineNo = lines.length;
    for (const k of activeFocus) {
      if (!focusByKey.has(k)) focusByKey.set(k, new Set());
      focusByKey.get(k).add(lineNo);
    }
    for (const k of activeAdd) {
      if (!addByKey.has(k)) addByKey.set(k, new Set());
      addByKey.get(k).add(lineNo);
    }
  }

  while (lines.length && lines[lines.length - 1] === '') lines.pop();
  return { name, path, lines, focusByKey, addByKey };
}

const APP_FILES = [
  parseAnnotatedFile('lib.rs', 'docs/examples/first-app/src/lib.rs', libRaw),
  parseAnnotatedFile('model.rs', 'docs/examples/first-app/src/model.rs', modelRaw),
  parseAnnotatedFile('interface.rs', 'docs/examples/first-app/src/interface.rs', interfaceRaw),
  parseAnnotatedFile('motion.rs', 'docs/examples/first-app/src/motion.rs', motionRaw),
  parseAnnotatedFile('web.rs', 'docs/examples/first-app/src/web.rs', webRaw),
  parseAnnotatedFile('desktop.rs', 'docs/examples/first-app/examples/desktop.rs', desktopRaw),
];

const RECIPE_FILE = parseAnnotatedFile('recipes.rs', 'docs/examples/first-app/src/recipes.rs', recipesRaw);

// Preferred file ordering per chapter so the primary file appears first in focus view
const CHAPTER_FILE_ORDER = {
  1: ['lib.rs', 'desktop.rs'],
  2: ['model.rs', 'interface.rs', 'lib.rs'],
  3: ['model.rs', 'interface.rs', 'lib.rs'],
  4: ['interface.rs', 'lib.rs', 'model.rs'],
  5: ['motion.rs', 'interface.rs', 'lib.rs', 'model.rs'],
  6: ['model.rs', 'lib.rs', 'interface.rs', 'motion.rs'],
  7: ['web.rs', 'lib.rs', 'interface.rs', 'model.rs', 'motion.rs', 'desktop.rs'],
};

export function buildSourceBundle({ chapter, recipe }) {
  const key = recipe || `ch${chapter}`;
  let files;
  if (recipe) {
    files = [RECIPE_FILE, APP_FILES[0]];
  } else {
    const order = CHAPTER_FILE_ORDER[chapter] || [];
    const byName = new Map(APP_FILES.map(f => [f.name, f]));
    const ordered = [];
    for (const name of order) {
      if (byName.has(name)) ordered.push(byName.get(name));
    }
    for (const f of APP_FILES) {
      if (!ordered.includes(f)) ordered.push(f);
    }
    files = ordered;
  }

  const indexedFiles = files.map((file, index) => ({
    ...file,
    index,
    focusSet: file.focusByKey.get(key) || new Set(),
    addSet: file.addByKey.get(key) || new Set(),
  }));

  const focusRows = [];
  for (const file of indexedFiles) {
    if (!file.focusSet.size) continue;
    let prevLine = -10;
    for (let i = 0; i < file.lines.length; i++) {
      const lineNo = i + 1;
      if (!file.focusSet.has(lineNo)) continue;
      if (focusRows.length && (focusRows[focusRows.length - 1].file !== file || lineNo > prevLine + 1)) {
        focusRows.push({ gap: true, file, line: lineNo });
      }
      focusRows.push({
        gap: false,
        file,
        line: lineNo,
        text: file.lines[i],
        added: file.addSet.has(lineNo),
      });
      prevLine = lineNo;
    }
  }

  const codeRows = focusRows.filter(r => !r.gap);
  if (codeRows.length) {
    const nonEmpty = codeRows.filter(r => r.text.trim().length > 0);
    const minIndent = nonEmpty.length
      ? Math.min(...nonEmpty.map(r => r.text.length - r.text.trimStart().length))
      : 0;
    for (const r of codeRows) {
      r.text = r.text.trim() ? r.text.slice(minIndent) : '';
    }
    if (new Set(codeRows.map(r => r.file)).size > 1) {
      focusRows.unshift({ gap: true, file: codeRows[0].file, line: codeRows[0].line });
    }
    const htmlLines = highlightLines(codeRows.map(r => r.text).join('\n'));
    let idx = 0;
    for (const r of focusRows) {
      if (!r.gap) r.html = htmlLines[idx++] || '';
    }
  }

  for (const file of indexedFiles) {
    const htmlLines = highlightLines(file.lines.join('\n'));
    file.renderedRows = file.lines.map((text, idx) => {
      const lineNo = idx + 1;
      return {
        gap: false,
        file,
        line: lineNo,
        text,
        html: htmlLines[idx] || '',
        marked: file.focusSet.has(lineNo),
        added: file.addSet.has(lineNo),
      };
    });
  }

  return {
    key,
    files: indexedFiles,
    focusRows: codeRows.length ? focusRows : null,
  };
}
