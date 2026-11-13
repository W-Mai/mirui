// Liquid glass for the controls that float over the page. The material
// itself is CSS: a clear tint, blur, and a rim that catches light. Where the
// browser can run an SVG filter on what lies behind an element (Chromium),
// the edge also bends that light: a displacement map drawn here for the
// element's size pulls each pixel of the backdrop toward the middle, by more
// the nearer it is to the rim, like the thick edge of a lens. Other browsers
// keep the material without the bend.

const SVG = 'http://www.w3.org/2000/svg';

/**
 * Where the backdrop pixel at (x, y) of a rounded box is taken from, as an
 * offset from the pixel itself. Pixels deeper than `bezel` from the rim stay
 * put; at the rim the offset is `depth` pixels toward the middle, and it eases
 * off with the square of the way in.
 */
export function displacement(x, y, width, height, radius, bezel, depth) {
    const px = x + .5 - width / 2, py = y + .5 - height / 2;
    const qx = Math.abs(px) - (width / 2 - radius), qy = Math.abs(py) - (height / 2 - radius);
    const reach = Math.hypot(Math.max(qx, 0), Math.max(qy, 0)) + Math.min(Math.max(qx, qy), 0);
    const edge = radius - reach;
    if (edge < 0 || edge >= bezel) return [0, 0];
    let nx = 0, ny = 0;
    if (qx > 0 && qy > 0) {
        const length = Math.hypot(qx, qy);
        nx = Math.sign(px) * qx / length;
        ny = Math.sign(py) * qy / length;
    } else if (qx > qy) {
        nx = Math.sign(px);
    } else {
        ny = Math.sign(py);
    }
    const pull = depth * (1 - edge / bezel) ** 2;
    return [-nx * pull, -ny * pull];
}

/** The displacement of a box as an image: red and green carry the x and y offsets, 128 meaning none. */
export function lensMap(width, height, radius, bezel, depth) {
    const canvas = document.createElement('canvas');
    canvas.width = width;
    canvas.height = height;
    const context = canvas.getContext('2d');
    const image = context.createImageData(width, height);
    for (let y = 0; y < height; y++) {
        for (let x = 0; x < width; x++) {
            const [dx, dy] = displacement(x, y, width, height, radius, bezel, depth);
            const at = (y * width + x) * 4;
            image.data[at] = Math.round(127.5 + dx / depth * 127.5);
            image.data[at + 1] = Math.round(127.5 + dy / depth * 127.5);
            image.data[at + 2] = 128;
            image.data[at + 3] = 255;
        }
    }
    context.putImageData(image, 0, 0);
    return canvas.toDataURL();
}

// Chromium applies SVG filters to a backdrop; the others ignore or drop them.
const refracts = () => !!navigator.userAgentData?.brands?.some(({brand}) => brand === 'Chromium')
    && CSS.supports('backdrop-filter', 'url(#lens) blur(1px)');

let defs = null, serial = 0;
function filterDefs() {
    if (defs) return defs;
    const svg = document.createElementNS(SVG, 'svg');
    svg.setAttribute('width', '0');
    svg.setAttribute('height', '0');
    svg.setAttribute('aria-hidden', 'true');
    svg.style.position = 'absolute';
    defs = document.createElementNS(SVG, 'defs');
    svg.append(defs);
    document.body.append(svg);
    return defs;
}

/**
 * Lets the glass catch the pointer: a soft highlight follows it across the
 * surface (`--lx`, `--ly`) and fades in while it is over, and out again after.
 * Returns a function that undoes it.
 */
export function trackLight(element) {
    const move = event => {
        const box = element.getBoundingClientRect();
        element.style.setProperty('--lx', `${event.clientX - box.left}px`);
        element.style.setProperty('--ly', `${event.clientY - box.top}px`);
        element.style.setProperty('--spec', '.2');
    };
    const leave = () => {
        element.style.removeProperty('--spec');
    };
    element.addEventListener('pointermove', move);
    element.addEventListener('pointerleave', leave);
    return () => {
        element.removeEventListener('pointermove', move);
        element.removeEventListener('pointerleave', leave);
        for (const name of ['--lx', '--ly', '--spec']) element.style.removeProperty(name);
    };
}

/**
 * Makes `element` bend what lies behind it and keeps the bend fitted to its
 * size. The element's CSS names the material; this sets `--lens` for it.
 * Returns a function that undoes it.
 */
export function mountLens(element, {bezel = 20, depth = 14} = {}) {
    if (!refracts()) return () => {};
    const id = `lens-${serial++}`;
    const filter = document.createElementNS(SVG, 'filter');
    filter.setAttribute('id', id);
    filter.setAttribute('filterUnits', 'userSpaceOnUse');
    filter.setAttribute('color-interpolation-filters', 'sRGB');
    const map = document.createElementNS(SVG, 'feImage');
    map.setAttribute('result', 'map');
    map.setAttribute('preserveAspectRatio', 'none');
    const move = document.createElementNS(SVG, 'feDisplacementMap');
    move.setAttribute('in', 'SourceGraphic');
    move.setAttribute('in2', 'map');
    move.setAttribute('scale', String(2 * depth));
    move.setAttribute('xChannelSelector', 'R');
    move.setAttribute('yChannelSelector', 'G');
    filter.append(map, move);
    filterDefs().append(filter);
    let fitted = '';
    const fit = () => {
        const width = element.offsetWidth, height = element.offsetHeight;
        if (!width || !height || fitted === `${width}x${height}`) return;
        fitted = `${width}x${height}`;
        const radius = Math.min(parseFloat(getComputedStyle(element).borderTopLeftRadius) || 0, width / 2, height / 2);
        for (const [name, value] of [['x', 0], ['y', 0], ['width', width], ['height', height]]) {
            filter.setAttribute(name, String(value));
            map.setAttribute(name, String(value));
        }
        map.setAttribute('href', lensMap(width, height, radius, Math.min(bezel, radius || bezel), depth));
        element.style.setProperty('--lens', `url(#${id})`);
        element.classList.add('lens');
    };
    const observer = new ResizeObserver(fit);
    observer.observe(element);
    fit();
    return () => {
        observer.disconnect();
        filter.remove();
        element.classList.remove('lens');
        element.style.removeProperty('--lens');
    };
}
