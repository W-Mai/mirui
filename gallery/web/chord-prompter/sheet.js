// A bar as it is written on a lead sheet: each chord stands over the first
// letter or character of its words, on the same left edge, and the words the
// song marks as accents are underlined. Where a chord begins and which words
// are accents come from the slots the shared Rust parser produced; nothing
// here looks for either in the text.

import { neckNode } from './neck.js';

// One element for each slot of a bar: the chord, then its words with each
// accent in an element of its own.
export function slotNodes(slots, doc = document) {
    return slots.map(({index, chord, segments, root, tones}) => {
        const slot = doc.createElement('span');
        slot.className = 'slot';
        slot.dataset.slot = String(index);
        if (root !== undefined) { slot.dataset.root = String(root); slot.dataset.tones = String(tones); }
        const name = doc.createElement('span');
        name.className = 'name';
        name.textContent = chord;
        const words = doc.createElement('span');
        words.className = 'words';
        for (const [text, accented] of segments) {
            if (!accented) {
                words.append(doc.createTextNode(text));
                continue;
            }
            const accent = doc.createElement('span');
            accent.className = 'accent';
            accent.textContent = text;
            words.append(accent);
        }
        slot.append(name, words);
        return slot;
    });
}

// A line of plain text where there is no bar to show.
export function note(text, doc = document) {
    const element = doc.createElement('span');
    element.className = 'note';
    element.textContent = text;
    return element;
}

// Fills `sheet` with a bar, or with `empty` when there is none. A bar with
// chords but no words says so below them; a solo, a bar the song leaves
// without words on purpose, shows where to play it on the neck instead.
export function fillSheet(sheet, content, empty = '', doc = document) {
    sheet.replaceChildren(...(content ? slotNodes(content.slots, doc) : empty ? [note(empty, doc)] : []));
    if (content?.neck) sheet.append(neckNode(content.neck, doc));
    sheet.classList.toggle('solo', !!content?.neck);
    sheet.classList.toggle('instrumental', !!content && !content.lyric && !content.neck);
}

// The notes of the chord a slot's element stands for, for the neck.
export function slotTones(slot) {
    return slot?.dataset.root === undefined ? null : {root: Number(slot.dataset.root), mask: Number(slot.dataset.tones)};
}

// How much larger the bar in the middle of the strip is drawn than the rest.
export const CENTER_GROWTH = .42;
// The strip keeps this much clear below the middle, for its needle mark, and
// the sheet never shrinks past this part of its size.
const NEEDLE_ROOM = 56, LEAST_FIT = .45;

// Shrinks a bar's chords and words until the bar, grown to the middle of the
// strip, fits the strip's `height`, so that no chord or word is cut off. A
// smaller size wraps less, so the height does not fall in step with the size;
// the largest size that fits is searched for, `--fit` being the part of the
// size kept.
export function fitSheet(card, sheet, height) {
    sheet.style.removeProperty('--fit');
    if (card.hidden || height <= 0) return;
    const room = (height - NEEDLE_ROOM) / (1 + CENTER_GROWTH) - (card.offsetHeight - sheet.offsetHeight);
    if (sheet.offsetHeight <= room) return;
    let fits = LEAST_FIT, tooBig = 1;
    for (let pass = 0; pass < 6; pass++) {
        const size = (fits + tooBig) / 2;
        sheet.style.setProperty('--fit', size.toFixed(3));
        if (sheet.offsetHeight <= room) fits = size; else tooBig = size;
    }
    sheet.style.setProperty('--fit', fits.toFixed(3));
}
