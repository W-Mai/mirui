const releasePattern = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;

export function isRelease(version) {
  return typeof version === 'string' && releasePattern.test(version);
}

export function compareReleases(left, right) {
  for (const [index, part] of left.split('.').entries()) {
    const a = BigInt(part);
    const b = BigInt(right.split('.')[index]);
    if (a !== b) return a > b ? 1 : -1;
  }
  return 0;
}

export function versionBase(version) {
  if (version !== 'dev' && !isRelease(version)) throw new Error(`Invalid documentation version: ${version}`);
  return `/docs/${version}/`;
}

export function pageKey(path) {
  return path.replace(/^\/+/, '').replace(/(?:^|\/)index\.html$/, '').replace(/\/+$/, '');
}

export function chapterTarget(index, current, language, hash = '') {
  const pages = index.pages;
  const key = pageKey(current).replace(/^zh(?:\/|$)/, '');
  const prefix = language === 'zh' ? 'zh/' : '';
  const localized = suffix => pageKey(prefix + suffix);
  let reason = '';
  let destination = localized(key);
  if (Object.hasOwn(pages, destination + '.html')) destination += '.html';
  if (!Object.hasOwn(pages, destination)) {
    reason = 'page';
    if (prefix && Object.hasOwn(pages, key)) {
      reason = 'language';
      destination = key;
    } else {
      const sections = key.split('/').slice(0, -1);
      while (sections.length && !Object.hasOwn(pages, localized(sections.join('/')))) sections.pop();
      destination = localized(sections.join('/'));
      if (!Object.hasOwn(pages, destination)) destination = '';
    }
  }
  let fragment = '';
  if (hash && reason !== 'page') {
    const raw = hash.replace(/^#/, '');
    let id = raw;
    try {
      id = decodeURIComponent(raw);
    } catch {
      // Malformed fragments fall back to their literal value.
    }
    if (pages[destination]?.includes(id)) fragment = '#' + encodeURIComponent(id);
    else if (!reason) reason = 'anchor';
  }
  const file = destination && (destination.endsWith('.html') ? destination : destination + '/');
  const query = reason ? `?reading=${reason}` : '';
  return versionBase(index.version) + file + query + fragment;
}
