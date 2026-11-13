// Where to play a solo: a box of a pentatonic scale on the neck, drawn the way
// a tab runs, the first string on top. The box itself comes from the shared
// Rust code, as the frets and pitch classes of two notes on every string; this
// only draws them, and lights the notes of the chord that sounds.

const SVG = 'http://www.w3.org/2000/svg';
// The strings from the low E up, as a guitarist names them.
const LETTERS = ['E', 'A', 'D', 'G', 'B', 'e'];
const FRETS = 5, CELL = 34, GAP = 17, LEFT = 18, TOP = 22, FOOT = 20;

function element(name, attributes, doc) {
    const node = doc.createElementNS(SVG, name);
    for (const [key, value] of Object.entries(attributes)) node.setAttribute(key, String(value));
    return node;
}

// The box as an SVG: the fret numbers over the frets, a letter for each
// string, every other fret on a lighter cell, and a dot for each note.
export function neckNode(neck, doc = document) {
    const width = LEFT + CELL * FRETS + 6, height = TOP + GAP * LETTERS.length + FOOT;
    const high = neck.high === null || neck.high === undefined ? '' : `, and again at fret ${neck.high}`;
    const svg = element('svg', {
        class: 'neck', viewBox: `0 0 ${width} ${height}`, role: 'img',
        'aria-label': `Pentatonic box from fret ${neck.low}${high}`,
    }, doc);
    for (let column = 0; column < FRETS; column++) {
        const fret = neck.low + column, x = LEFT + column * CELL;
        if (fret % 2 === 1) svg.append(element('rect', {class: 'fret-cell', x, y: TOP, width: CELL, height: GAP * LETTERS.length}, doc));
        const number = element('text', {class: column === 0 ? 'fret first' : 'fret', x: x + CELL / 2, y: TOP - 9}, doc);
        number.textContent = String(fret);
        svg.append(number);
    }
    for (let row = 0; row < LETTERS.length; row++) {
        const y = TOP + row * GAP + GAP / 2;
        svg.append(element('line', {class: 'string', x1: LEFT, x2: LEFT + CELL * FRETS, y1: y, y2: y}, doc));
        const letter = element('text', {class: 'letter', x: LEFT - 9, y: y + 4}, doc);
        letter.textContent = LETTERS[LETTERS.length - 1 - row];
        svg.append(letter);
    }
    for (const [string, frets] of neck.frets.entries()) {
        for (const [slot, fret] of frets.entries()) {
            const column = fret - neck.low;
            if (column < 0 || column >= FRETS) continue;
            svg.append(element('circle', {
                class: 'dot scale', 'data-note': neck.notes[string][slot],
                cx: LEFT + column * CELL + CELL / 2, cy: TOP + (LETTERS.length - 1 - string) * GAP + GAP / 2, r: 4,
            }, doc));
        }
    }
    if (neck.high !== null && neck.high !== undefined) {
        const note = element('text', {class: 'again', x: LEFT, y: height - 5}, doc);
        note.textContent = `also fret ${neck.high} to ${neck.high + FRETS - 1}`;
        svg.append(note);
    }
    return svg;
}

// What a note of the box is to a chord: its root, another note of its triad,
// or only a note of the scale. `tones` is the chord's `{root, mask}`; without
// one every note is plain.
export function role(note, tones) {
    if (!tones) return 'scale';
    if (note === tones.root) return 'root';
    return (tones.mask >> note) & 1 ? 'tone' : 'scale';
}

// Lights the notes of the chord that sounds.
export function lightNeck(svg, tones) {
    for (const dot of svg.querySelectorAll('.dot')) {
        dot.setAttribute('class', `dot ${role(Number(dot.dataset.note), tones)}`);
    }
}
