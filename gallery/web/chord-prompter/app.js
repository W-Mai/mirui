import { ChordLink, SERVICE, PATTERNS } from './protocol.js';
import { TICKS_PER_BEAT, barContent, choose, follow, sectionAt, sectionStarts, steer, stripBar } from './presentation.js';
import { CENTER_GROWTH, fillSheet, fitSheet, slotTones } from './sheet.js';
import { lightNeck } from './neck.js';
import { mountLens, trackLight } from './glass.js';
import { DeviceSync } from './sync.js';

const $ = id => document.getElementById(id);
const reducedMotion = matchMedia('(prefers-reduced-motion: reduce)');
const mobileLayout = matchMedia('(max-width: 800px)');
const appearanceQuery = matchMedia('(prefers-color-scheme: dark)');
// A seek argument that drops the queued jump; see the device protocol.
const CANCEL_JUMP = 0xffffffff;
// The display accepts start delays up to five seconds.
const COUNT_IN_LIMIT = 5000;
let appearancePicked = false;
try { appearancePicked = !!localStorage.getItem('mirui-theme'); } catch { /* Use the head-resolved appearance. */ }
function applyAppearance(dark, persist = false) {
    document.documentElement.dataset.theme = dark ? 'dark' : 'light';
    const label = dark ? 'Switch to light theme' : 'Switch to dark theme';
    $('appearance').title = label;
    $('appearance').setAttribute('aria-label', label);
    document.querySelector('meta[name="theme-color"]').content = getComputedStyle(document.documentElement).getPropertyValue('--bg').trim();
    if (persist) {
        appearancePicked = true;
        try { localStorage.setItem('mirui-theme', dark ? 'dark' : 'light'); } catch { /* Appearance still applies to this page. */ }
    }
}
$('appearance').onclick = () => applyAppearance(document.documentElement.dataset.theme !== 'dark', true);
appearanceQuery.addEventListener('change', event => { if (!appearancePicked) applyAppearance(event.matches); });
window.addEventListener('storage', event => {
    if (event.key !== 'mirui-theme') return;
    appearancePicked = !!event.newValue;
    applyAppearance(event.newValue ? event.newValue === 'dark' : appearanceQuery.matches);
});
applyAppearance(document.documentElement.dataset.theme !== 'light');

function savedDraft() {
    try {
        const source = sessionStorage.getItem('mirui-chord-draft');
        return source !== null && new TextEncoder().encode(source).length <= 8192 ? source : null;
    } catch { return null; }
}
function saveDraft() {
    const source = $('source').value;
    if (new TextEncoder().encode(source).length > 8192) return;
    try { sessionStorage.setItem('mirui-chord-draft', source); } catch { /* Editing does not require storage. */ }
}
try { $('count-in').checked = localStorage.getItem('mirui-chord-count-in') !== 'off'; } catch { /* Counts in by default. */ }
$('count-in').onchange = () => {
    try { localStorage.setItem('mirui-chord-count-in', $('count-in').checked ? 'on' : 'off'); } catch { /* Applies to this page. */ }
};
const errorText = error => String(error?.message ?? error);
let bindings, patterns = [], preview, previewData, draftData, deviceData, deviceGeneration;
let engineReady = false;
let sample = null, connecting = false, uploading = false, previewMode = true;
let beatCount = 0, rhythmIndex = -1, activeSlot = -1, revision = 0, editTimer, scrubbing = false, scrubPointer = false;
// The latest frame's playback, read by the controls.
let view = null;
// When a browser count-in ends, in host milliseconds.
let countInUntil = 0;
// Strip bookkeeping: played bars behind the current one, what is drawn, and
// the offsets of a slide after a seek and of a finger dragging the bars.
let trail = [], shown = {identity: '', bar: null, jumpBar: null}, rendered = {key: ''}, slide = null, drag = null;
const barCards = [...document.querySelectorAll('.bar-card')];
const currentCard = $('current-card');
const barWindow = document.querySelector('.bar-window');
let messageTimer;
// Notes clear themselves; errors stay until the next message.
function message(text = '', error = false) {
    clearTimeout(messageTimer);
    $('message').textContent = text;
    $('message').classList.toggle('error', error);
    if (text && !error) messageTimer = setTimeout(() => { $('message').textContent = ''; }, 2600);
}
const link = new ChordLink(() => {
    deviceSync.clear();
    sample = null; previewMode = true; deviceData = null; deviceGeneration = undefined; refreshConnection();
    message('Disconnected. Display playback continues.');
});
const deviceSync = new DeviceSync(link, {onSample: associateDeviceSample,
    onError: error => { message(errorText(error), true); refreshConnection(); }, visible: !document.hidden});
const usingDevice = () => link.connected && !previewMode;

function refreshConnection() {
    deviceSync.setActive(usingDevice());
    $('connection').textContent = link.connected ? 'Connected' : 'Not connected';
    $('lamp').classList.toggle('online', link.connected);
    $('workbench').classList.toggle('device', usingDevice());
    $('mode').textContent = usingDevice() ? 'Display' : 'Browser preview';
    $('connect').textContent = connecting ? 'Connecting…' : link.connected ? 'Disconnect' : mobileLayout.matches ? 'Connect' : 'Connect display';
    $('connect').disabled = connecting || !engineReady;
    for (const button of document.querySelectorAll('[data-ready]')) button.disabled = !engineReady || (!preview && !usingDevice()) || connecting;
    $('preview').disabled = !engineReady || !draftData || connecting || uploading;
    $('preview').setAttribute('aria-pressed', String(engineReady && !usingDevice()));
    for (const button of document.querySelectorAll('[data-device]')) button.disabled = !engineReady || !link.connected || connecting || uploading || !draftData;
}
async function run(operation) {
    try { return await operation(); }
    catch (error) { message(errorText(error), true); refreshConnection(); }
}
function editor(open) {
    const focusInside = $('editor').contains(document.activeElement);
    $('workbench').classList.toggle('editor-open', open);
    $('editor-toggle').setAttribute('aria-expanded', String(open));
    $('editor-toggle').setAttribute('aria-label', open ? 'Close editor' : 'Open editor');
    $('editor-toggle').title = open ? 'Close editor' : 'Open editor';
    $('editor').inert = !open;
    document.querySelector('.music').inert = open && mobileLayout.matches;
    if (!open && focusInside) $('editor-toggle').focus();
}
mobileLayout.addEventListener('change', () => {
    document.querySelector('.music').inert = mobileLayout.matches && $('workbench').classList.contains('editor-open');
    refreshConnection();
});
$('editor-toggle').onclick = () => editor(!$('workbench').classList.contains('editor-open'));
$('editor-close').onclick = () => editor(false);
$('fullscreen').onclick = () => run(async () => {
    if (document.fullscreenElement) await document.exitFullscreen();
    else await document.documentElement.requestFullscreen();
});
document.addEventListener('fullscreenchange', () => {
    const active = !!document.fullscreenElement;
    const label = active ? 'Exit fullscreen' : 'Enter fullscreen';
    $('fullscreen').setAttribute('aria-pressed', String(active));
    $('fullscreen').setAttribute('aria-label', label);
    $('fullscreen').title = label;
});

// Keys and page-turn pedals. A control clicked with the pointer keeps focus
// but not its keys; a control reached with Tab keeps the keys it uses, and
// text fields keep every key.
const STEPS = {ArrowRight: 1, ArrowDown: 1, PageDown: 1, ArrowLeft: -1, ArrowUp: -1, PageUp: -1};
let keyboardFocus = false;
window.addEventListener('pointerdown', () => { keyboardFocus = false; }, true);
function ownsKey(target, code) {
    if (target.matches?.('textarea, select, [contenteditable], input:not([type=range]):not([type=checkbox]):not([type=file])')) return true;
    if (!keyboardFocus) return false;
    if (target.matches?.('input[type=range]')) return code in STEPS || code === 'Home' || code === 'End';
    return (code === 'Space' || code === 'Enter') && !!target.closest?.('a,button,summary,label,input');
}
window.addEventListener('keydown', event => {
    if (event.code === 'Tab') { keyboardFocus = true; return; }
    if (event.code === 'Escape' && $('workbench').classList.contains('editor-open')) { editor(false); return; }
    if (event.altKey || event.ctrlKey || event.metaKey || ownsKey(event.target, event.code)) return;
    // A clicked control would light its focus ring on this key; it gives up focus instead.
    if (!keyboardFocus && event.target !== document.body) event.target.blur?.();
    if (event.code === 'Space') {
        event.preventDefault();
        if (!event.repeat) $('play').click();
    } else if (STEPS[event.code]) {
        event.preventDefault();
        stepBy(STEPS[event.code], event.shiftKey);
    } else if (event.code === 'Home') {
        event.preventDefault();
        pick(0);
    } else if (event.code === 'Escape' && view && view.jump !== null) {
        go('cancel');
    } else if (event.key === '+' || event.key === '=') {
        nudgeTempo(1);
    } else if (event.key === '-' || event.key === '_') {
        nudgeTempo(-1);
    }
});

function associateDeviceSample(next) {
    sample = next;
    if (deviceGeneration !== next.generation) {
        deviceData = null;
        try {
            const saved = JSON.parse(sessionStorage.getItem('mirui-chord-active') ?? 'null');
            if (saved?.generation === next.generation && saved.device === (link.device?.id ?? '')
                && Array.isArray(saved.data?.sections) && saved.data.slots?.every(slot => Array.isArray(slot.segments))) deviceData = saved.data;
        } catch { /* The display remains usable without stored editor context. */ }
        deviceGeneration = next.generation;
    }
    deviceSync.setScore(deviceData ?? null);
}
async function poll(allowBlocked = false) {
    try { await deviceSync.refresh({allowBlocked}); }
    catch (error) { message(errorText(error), true); }
    finally { refreshConnection(); }
}
$('connect').onclick = () => run(async () => {
    if (link.connected) {
        deviceSync.clear(); link.close(); sample = null; deviceData = null; deviceGeneration = undefined; previewMode = true;
        refreshConnection(); message('Disconnected. Display playback continues.'); return;
    }
    if (!navigator.bluetooth) throw new Error('Open in Chrome or Arc on desktop, or Chrome on Android, to connect the display.');
    if (!isSecureContext) throw new Error('Bluetooth requires HTTPS or localhost.');
    connecting = true; refreshConnection();
    try {
        const device = await navigator.bluetooth.requestDevice({ filters: [{ namePrefix: 'MIRUI Chord' }], optionalServices: [SERVICE] });
        const next = await link.open(device);
        preview?.pause(performance.now()); previewMode = false; rendered.key = ''; countInUntil = 0;
        deviceSync.accept(next);
        message(deviceData ? '' : 'Display connected · song source is not synced');
    } finally { connecting = false; refreshConnection(); }
});

const barTicks = () => (view?.beats ?? (usingDevice() ? deviceSync.sample?.beats : previewData?.beats) ?? 4) * TICKS_PER_BEAT;
async function control(opcode, arg = 0, boundary = 0) {
    if (!engineReady || (!preview && !usingDevice())) throw new Error('Load a song first');
    if (usingDevice()) {
        const receipt = await link.command(opcode, arg, boundary);
        deviceSync.noteControl(receipt, opcode === 5 && arg !== CANCEL_JUMP ? arg * barTicks() : null);
        await poll(true);
        return;
    }
    const now = performance.now();
    if (opcode === 1) preview.play(now + arg);
    else if (opcode === 2) preview.pause(now);
    else if (opcode === 3) preview.stop();
    else if (opcode === 4) preview.tempo(now, arg);
    else if (opcode === 5) { if (arg === CANCEL_JUMP) preview.cancel_jump(); else preview.seek(now, arg); }
    else if (opcode === 6) preview.rhythm(now, arg);
}
// One bar at the current tempo, shortened to fit the display's start delay.
function countInMs() {
    if (!$('count-in').checked || !view) return 0;
    const beat = 60_000_000 / view.bpm;
    return Math.round(Math.max(1, Math.min(view.beats, Math.floor(COUNT_IN_LIMIT / beat))) * beat);
}
function togglePlay() {
    if (view?.running) { countInUntil = 0; return control(2); }
    const ms = countInMs();
    countInUntil = performance.now() + ms;
    return control(1, usingDevice() ? Math.max(500, ms) : ms);
}
$('play').onclick = () => run(togglePlay);
$('stop').onclick = () => { countInUntil = 0; run(() => control(3)); };

// A chosen bar plays next while playing, and is moved to at once otherwise.
// The choice shows in `view` at once, so quick repeated steps add up before
// the next frame or the display's receipt.
function go(target) {
    if (target === null || !view) return;
    if (view.playing) view.jump = target === 'cancel' ? null : target;
    else if (target !== 'cancel') { view.bar = target; view.offset = 0; }
    run(() => control(5, target === 'cancel' ? CANCEL_JUMP : target, 2));
}
const pick = bar => { if (view?.data) go(choose(view, Math.max(0, Math.min(view.bars - 1, bar)))); };
const stepBy = (delta, bySection = false) => { if (view?.data) go(steer(view, delta, bySection)); };

// Whole steps from the keys and buttons; typed tempos keep their decimals.
function nudgeTempo(delta) {
    if (!view) return;
    const value = Number($('tempo').value) || view.bpm / 1000;
    const next = Math.max(20, Math.min(300, delta > 0 ? Math.floor(value) + delta : Math.ceil(value) + delta));
    $('tempo').value = String(next);
    run(async () => {
        await control(4, next * 1000, 1);
        if (view?.playing) message(`Tempo ${next} · next beat`);
    });
}
$('tempo-down').onclick = event => nudgeTempo(event.shiftKey ? -5 : -1);
$('tempo-up').onclick = event => nudgeTempo(event.shiftKey ? 5 : 1);
$('tempo').onkeydown = event => {
    if (event.key !== 'ArrowUp' && event.key !== 'ArrowDown') return;
    event.preventDefault();
    nudgeTempo((event.key === 'ArrowUp' ? 1 : -1) * (event.shiftKey ? 5 : 1));
};
$('tempo-form').onsubmit = event => { event.preventDefault(); $('tempo').blur(); };
$('tempo').onchange = () => { if ($('tempo').validity.valid) run(() => control(4, Math.round(Number($('tempo').value) * 1000), 1)); };
$('seek-form').onsubmit = event => { event.preventDefault(); pick(Number($('bar').value) - 1); };
$('rhythm-form').onsubmit = event => {
    event.preventDefault();
    const index = Number($('rhythm').value);
    run(async () => {
        await control(6, index, 2);
        if (view?.playing) message(`Strumming ${PATTERNS[index] ?? 'from song'} · next bar`);
    });
};
let inspectedBar = null, inspectedIdentity = '';
function showBarPreview(bar) {
    inspectedBar = bar;
    inspectedIdentity = '';
    refreshBarPreview();
}
function hideBarPreview() {
    inspectedBar = null;
    $('bar-preview').hidden = true;
}
function refreshBarPreview() {
    if (inspectedBar === null) return;
    const data = usingDevice() ? deviceData : previewData;
    const identity = `${usingDevice() ? `device-${deviceGeneration}` : `preview-${revision}`}:${inspectedBar}:${!!data}`;
    if (identity === inspectedIdentity) return;
    inspectedIdentity = identity;
    $('bar-preview').hidden = false;
    for (const [offset, name] of [[0, 'hover-current'], [1, 'hover-next']]) {
        const content = barContent(data, inspectedBar + offset);
        const section = content ? sectionAt(data, inspectedBar + offset) : '';
        $(`${name}-label`).textContent = `Bar ${inspectedBar + offset + 1}${section ? ` · ${section}` : ''}`;
        fillSheet($(`${name}-sheet`), content, data ? 'End of song' : 'Song source not synced');
    }
}
const positionRange = $('position-range');
// The part of the track before the thumb is drawn in the accent colour.
function paintRange() {
    const min = Number(positionRange.min), span = Number(positionRange.max) - min;
    positionRange.style.setProperty('--progress', `${span > 0 ? (Number(positionRange.value) - min) / span * 100 : 0}%`);
}
positionRange.setAttribute('aria-describedby', 'bar-preview');
positionRange.onpointermove = event => {
    const rect = positionRange.getBoundingClientRect();
    const ratio = Math.max(0, Math.min(1, (event.clientX - rect.left - 8) / Math.max(1, rect.width - 16)));
    showBarPreview(Math.round(ratio * (Number(positionRange.max) - 1)));
};
positionRange.onpointerleave = () => {
    if (document.activeElement === positionRange) showBarPreview(Number(positionRange.value) - 1);
    else hideBarPreview();
};
positionRange.onpointerdown = () => { scrubPointer = true; };
positionRange.onfocus = () => showBarPreview(Number(positionRange.value) - 1);
positionRange.onblur = hideBarPreview;
positionRange.onkeydown = event => { if (event.key === 'Escape') hideBarPreview(); };
positionRange.oninput = () => { scrubbing = true; positionKey = ''; paintRange(); $('position').textContent = `Bar ${positionRange.value}`; showBarPreview(Number(positionRange.value) - 1); };
positionRange.onchange = () => {
    pick(Number(positionRange.value) - 1);
    scrubbing = false;
    // Hand the keys back to playback after a pointer scrub.
    if (scrubPointer) positionRange.blur();
    scrubPointer = false;
};
for (const [index, name] of PATTERNS.entries()) {
    const option = document.createElement('option'); option.value = String(index); option.textContent = name; $('rhythm').append(option);
}

function byteCount() { $('bytes').textContent = `${new TextEncoder().encode($('source').value).length} / 8192 bytes`; }
// `keep` continues the current preview in the edited song instead of
// starting it over.
function parseEditor(activate = false, keep = false) {
    byteCount();
    if (!engineReady) return;
    let candidate;
    try {
        candidate = new bindings.ChordPreview($('source').value);
        draftData = JSON.parse(candidate.score_json());
        $('validation').textContent = `${draftData.bars} bars`; $('validation').classList.remove('error');
        if (activate) {
            if (keep && preview) candidate.adopt(performance.now(), preview);
            else countInUntil = 0;
            preview?.free(); preview = candidate; candidate = null;
            previewData = draftData; previewMode = true; revision++; rendered.key = '';
            if (!keep) message('');
        }
    } catch (error) {
        draftData = null;
        $('validation').textContent = errorText(error); $('validation').classList.add('error');
        if (activate && !keep) message(errorText(error), true);
    } finally { candidate?.free(); refreshConnection(); }
}
// While previewing, a valid edit shows at once from the same place.
$('source').oninput = () => { byteCount(); clearTimeout(editTimer); editTimer = setTimeout(() => parseEditor(!usingDevice(), true), 250); };
$('preview').onclick = () => parseEditor(true, true);
$('file').onchange = () => run(async () => {
    const file = $('file').files[0]; if (!file) return;
    if (file.size > 8192) throw new Error('Song sheet exceeds 8192 bytes');
    $('source').value = new TextDecoder('utf-8', { fatal: true }).decode(await file.arrayBuffer());
    parseEditor(!usingDevice());
});
$('upload').onclick = () => run(async () => {
    if (uploading) return;
    const source = $('source').value;
    const candidate = new bindings.ChordPreview(source);
    const data = JSON.parse(candidate.score_json()); candidate.free();
    const total = new TextEncoder().encode(source).length;
    const epoch = link.epoch;
    uploading = true; deviceSync.setBlocked(true); refreshConnection();
    let announced = -1;
    $('transfer').hidden = false; $('workbench').classList.add('has-transfer'); $('transfer').dataset.state = 'sending';
    $('upload').textContent = 'Sending…'; $('progress').value = 0;
    $('transfer-label').textContent = `Sending 0 / ${total} bytes · 0%`;
    $('transfer-announcement').textContent = 'Uploading song';
    try {
        const receipt = await link.upload(source, ratio => {
            $('progress').value = ratio;
            $('transfer-label').textContent = ratio === 1 ? 'Validating on display…' : `Sending ${Math.round(ratio * total)} / ${total} bytes · ${Math.round(ratio * 100)}%`;
            const bucket = Math.floor(ratio * 10);
            if (bucket !== announced) {
                announced = bucket;
                $('transfer-announcement').textContent = ratio === 1 ? 'Validating on display' : `Upload ${bucket * 10}%`;
            }
        });
        deviceSync.clear(); sample = null;
        deviceData = data; deviceGeneration = receipt.generation; previewMode = false;
        preview?.pause(performance.now()); rendered.key = ''; countInUntil = 0;
        try { sessionStorage.setItem('mirui-chord-active', JSON.stringify({ device: link.device?.id ?? '', generation: deviceGeneration, data })); } catch { /* Storage is optional. */ }
        $('transfer').dataset.state = 'complete'; $('transfer-label').textContent = `Loaded ${total} bytes · 100%`;
        $('transfer-announcement').textContent = 'Song loaded';
        message('');
    } catch (error) {
        if (epoch !== link.epoch) return;
        $('transfer').dataset.state = 'error'; $('transfer-label').textContent = `Upload failed: ${errorText(error)}`;
        $('transfer-announcement').textContent = $('transfer-label').textContent;
        throw error;
    } finally {
        uploading = false; deviceSync.setBlocked(false);
        $('upload').textContent = 'Send to display'; refreshConnection();
        if (epoch === link.epoch) await poll();
    }
});

// Tap a bar to play it next; swipe the bars to step. The bars follow the
// finger a little and settle back.
const stepWidth = () => parseFloat(getComputedStyle(barWindow).getPropertyValue('--bar-step')) || 220;
const dragShift = () => drag?.moved && !reducedMotion.matches ? Math.max(-.6, Math.min(.6, drag.dx / stepWidth() * .45)) : 0;
barWindow.addEventListener('pointerdown', event => {
    if (event.button !== 0 || !view?.data) return;
    drag = {id: event.pointerId, x: event.clientX, y: event.clientY, dx: 0, dy: 0, moved: false, card: event.target.closest('.bar-card')};
    barWindow.setPointerCapture(event.pointerId);
});
barWindow.addEventListener('pointermove', event => {
    if (drag?.id !== event.pointerId) return;
    drag.dx = event.clientX - drag.x; drag.dy = event.clientY - drag.y;
    drag.moved ||= Math.hypot(drag.dx, drag.dy) > 8;
});
function releaseDrag(event, commit) {
    if (drag?.id !== event.pointerId) return;
    const {moved, dx, dy, card} = drag, shift = dragShift();
    drag = null;
    if (shift) startSlide(shift);
    if (!commit) return;
    if (!moved && card?.dataset.bar) pick(Number(card.dataset.bar));
    else if (Math.abs(dx) > 40 && Math.abs(dx) > Math.abs(dy) * 1.5) stepBy(dx < 0 ? 1 : -1);
}
barWindow.addEventListener('pointerup', event => releaseDrag(event, true));
barWindow.addEventListener('pointercancel', event => releaseDrag(event, false));

// A seek moves the bars in from the side they came from; motion that only
// changes a queued jump crossfades the bars ahead.
const ease = t => 1 - (1 - t) ** 3;
function slideShift(now) {
    if (!slide) return 0;
    const t = (now - slide.start) / 320;
    if (t >= 1) { slide = null; return 0; }
    return slide.from * (1 - ease(t));
}
function startSlide(from) {
    if (reducedMotion.matches) return;
    const now = performance.now();
    slide = {from: Math.max(-1.6, Math.min(1.6, from + slideShift(now))), start: now};
}
function restart(card, name) {
    card.classList.remove(name);
    void card.offsetWidth;
    card.classList.add(name);
}
for (const card of barCards) card.addEventListener('animationend', () => card.classList.remove('swap'));
// The bars are fitted to the strip's height; a new height fits them again.
new ResizeObserver(() => { rendered = {key: ''}; }).observe(barWindow);
// The control bar and the preview of a bar are liquid glass.
mountLens($('controller'), {bezel: 26, depth: 18});
trackLight($('controller'));
mountLens($('bar-preview'), {bezel: 18, depth: 12});

function slotAt(data, tick) {
    let lo = 0, hi = data.slots.length;
    while (lo < hi) { const mid = (lo + hi) >>> 1; if (data.slots[mid].tick <= tick) lo = mid + 1; else hi = mid; }
    return Math.max(0, lo - 1);
}
function renderStrip(data, index, layout) {
    if (!data) {
        if (rendered.key !== 'unknown') {
            rendered = {key: 'unknown'};
            for (const card of barCards) {
                const sheet = card.querySelector('.sheet');
                sheet.style.removeProperty('--fit');
                fillSheet(sheet, null, card === currentCard ? 'Song not synced' : '');
                card.hidden = Number(card.dataset.offset) !== 0;
                card.dataset.bar = '';
                card.classList.remove('jump');
                card.querySelector('.bar-number').textContent = '';
            }
            $('section-name').textContent = '';
            $('song-title').textContent = 'Chord Prompter';
        }
        return;
    }
    const key = `${layout.identity}:${layout.bar}:${layout.trail.join()}:${layout.jumpBar}:${layout.ahead}`;
    if (key === rendered.key) { markActiveSlot(index); return; }
    const retarget = rendered.identity === layout.identity && rendered.bar === layout.bar && rendered.jumpBar !== layout.jumpBar;
    for (const card of barCards) {
        const offset = Number(card.dataset.offset), bar = stripBar(layout, offset);
        const content = barContent(data, bar);
        card.hidden = !content;
        card.dataset.bar = content ? String(bar) : '';
        card.classList.toggle('jump', layout.jumpBar !== null && offset === layout.ahead);
        card.querySelector('.bar-number').textContent = content ? String(content.number).padStart(2, '0') : '';
        fillSheet(card.querySelector('.sheet'), content);
        fitSheet(card, card.querySelector('.sheet'), barWindow.clientHeight);
        if (retarget && offset >= layout.ahead && !reducedMotion.matches) restart(card, 'swap');
    }
    activeSlot = -1; markActiveSlot(index);
    $('song-title').textContent = data.title;
    document.title = `${data.title} · Chord Prompter`;
    rendered = {key, identity: layout.identity, bar: layout.bar, jumpBar: layout.jumpBar};
}
// The slot playing is lit and the ones before it recede, as on the display.
function markActiveSlot(index) {
    if (activeSlot === index) return;
    activeSlot = index;
    for (const slot of currentCard.querySelectorAll('[data-slot]')) {
        const at = Number(slot.dataset.slot);
        slot.classList.toggle('active', at === index);
        slot.classList.toggle('past', at < index);
    }
    // A solo lights the notes of the chord that sounds in its box.
    const neck = currentCard.querySelector('.neck');
    if (neck) lightNeck(neck, slotTones(currentCard.querySelector(`[data-slot="${index}"]`)));
}
function moveCards(phase, hasNext, shift) {
    const travel = reducedMotion.matches || !hasNext ? 0 : phase;
    for (const card of barCards) {
        const relative = Number(card.dataset.offset) - travel + shift;
        const emphasis = bindings.chord_center_emphasis(Math.round(relative * 256)) / 256;
        card.style.transform = `translate(calc(${relative} * var(--bar-step) - 50%), -50%) scale(${1 + emphasis * CENTER_GROWTH})`;
        card.style.opacity = String((relative < -.5 ? .45 : .75) + emphasis * .25);
    }
}
let marksKey = '';
function renderMarks(data, jumpBar) {
    const key = `${rendered.identity}:${data?.bars}:${jumpBar}`;
    if (key === marksKey) return;
    marksKey = key;
    const span = Math.max(1, (data?.bars ?? 1) - 1), marks = [];
    for (const start of data ? sectionStarts(data).filter(start => start > 0) : []) {
        const tick = document.createElement('i'); tick.className = 'section'; tick.style.left = `${start / span * 100}%`; marks.push(tick);
    }
    if (jumpBar !== null) {
        const mark = document.createElement('i'); mark.className = 'jump'; mark.style.left = `${jumpBar / span * 100}%`; marks.push(mark);
    }
    $('scrub-marks').replaceChildren(...marks);
}
let positionKey = '';
function showPosition(bar, data, state, jumpBar) {
    const queued = state.pending && jumpBar === null ? ' · queued' : '';
    const key = `${bar}:${jumpBar}:${data?.bars}:${queued}`;
    if (key === positionKey) return;
    positionKey = key;
    const parts = [`Bar ${bar + 1}`];
    if (jumpBar !== null) {
        const to = document.createElement('span'); to.className = 'to'; to.textContent = ` → ${jumpBar + 1}`; parts.push(to);
    }
    parts.push(` / ${data?.bars ?? '—'}${queued}`);
    $('position').replaceChildren(...parts);
}
const setText = (element, text) => { if (element.textContent !== text) element.textContent = text; };
function frame() {
    refreshBarPreview();
    const now = performance.now();
    const remote = usingDevice();
    const data = remote ? deviceData : previewData;
    let state;
    if (remote && sample) {
        state = deviceSync.state(now);
    } else if (!remote && preview) {
        const tick = preview.tick(now), at = preview.jump_at;
        const armed = preview.playing && now < countInUntil;
        state = { tick, bpm: preview.bpm, playing: preview.playing, beats: previewData.beats, unit: previewData.unit, pattern: preview.pattern,
            pending: preview.pending, armed, startsAt: armed ? countInUntil : null, jump: at === undefined ? null : {at, to: preview.jump_to}, stale: false };
    }
    if (state) {
        const ticksPerBar = state.beats * TICKS_PER_BEAT;
        const maxTick = data ? data.bars * ticksPerBar - 1 : Infinity;
        const tick = Math.min(state.tick, maxTick), beat = Math.floor(tick / TICKS_PER_BEAT) % state.beats, bar = Math.floor(tick / ticksPerBar);
        const moving = state.playing && !state.armed;
        const jumpBar = state.jump ? Math.floor(state.jump.to / ticksPerBar) : null;
        const ahead = state.jump ? Math.max(1, Math.round(state.jump.at / ticksPerBar) - bar) : 1;
        view = {data, bar, bars: data?.bars ?? 1, offset: tick % ticksPerBar, playing: moving, running: state.playing,
            jump: jumpBar, starts: sectionStarts(data), beats: state.beats, bpm: state.bpm};
        const identity = remote ? `device-${deviceGeneration}` : `preview-${revision}`;
        if (identity !== shown.identity) {
            trail = follow([], null, bar, false);
            slide = null;
        } else if (bar !== shown.bar) {
            const carried = moving && (bar === shown.bar + 1 || bar === shown.jumpBar);
            trail = follow(trail, shown.bar, bar, moving, shown.jumpBar);
            if (!carried) startSlide(bar - shown.bar);
        }
        shown = {identity, bar, jumpBar};
        if (beatCount !== state.beats) {
            $('beats').replaceChildren(...Array.from({length: state.beats}, (_, index) => {const dot = document.createElement('i'); dot.textContent=String(index+1); return dot;})); beatCount = state.beats;
            $('strum-track').style.setProperty('--beats', String(state.beats));
            // The count of the bar: a number on each beat and a plus between beats.
            $('count').replaceChildren(...Array.from({length: state.beats * 2}, (_, half) => {
                const mark = document.createElement(half % 2 ? 'i' : 'b');
                mark.textContent = half % 2 ? '+' : String(half / 2 + 1);
                mark.style.left = `${half / (state.beats * 2) * 100}%`;
                return mark;
            }));
        }
        // A count-in lights the beats before the music starts.
        let lit = state.stale ? -1 : beat;
        if (state.armed) lit = state.startsAt === null ? -1 : state.beats - Math.ceil((state.startsAt - now) * state.bpm / 60_000_000);
        for (const [index, dot] of [...$('beats').children].entries()) dot.classList.toggle('active', index === lit);
        $('beats').classList.toggle('counting', !!state.armed);
        $('meter').textContent = `${state.beats} / ${state.unit}`;
        $('play').setAttribute('aria-pressed', String(state.playing));
        $('play').setAttribute('aria-label', state.playing ? 'Pause' : 'Play');
        $('play').title = state.playing ? 'Pause (Space)' : 'Play (Space)';
        if (!scrubbing) { showPosition(bar, data, state, jumpBar); $('position-range').max = String(data?.bars ?? 128); $('position-range').value = String(bar + 1); paintRange(); }
        if (document.activeElement !== $('tempo') && !state.pending) $('tempo').value = String(state.bpm / 1000);
        renderStrip(data, data ? slotAt(data, tick) : 0, {identity, bar, trail, jumpBar, ahead});
        currentCard.classList.toggle('running', moving);
        renderMarks(data, jumpBar);
        setText($('section-name'), state.armed ? 'Count-in' : data ? sectionAt(data, bar) : '');
        document.querySelector('.section-heading').classList.toggle('counting', !!state.armed);
        if (rhythmIndex !== state.pattern) {
            rhythmIndex = state.pattern;
            $('attacks').replaceChildren(...(patterns[rhythmIndex] ?? []).map(attack => {
                // Strings run as on a tab: the sixth, low E, at the bottom. A down stroke
                // sweeps from the sixth string to the first, so its arrow points up and is
                // the heavy one; an up stroke points down and is the light one.
                const mark = document.createElement('span');
                mark.className = `attack ${attack.stroke}${attack.velocity >= 90 ? ' strong' : ''}`;
                mark.style.left = `${attack.offset / 4}%`;
                mark.style.setProperty('--low', String(attack.low));
                mark.style.setProperty('--high', String(attack.high));
                mark.title = attack.stroke === 'pick' ? `String ${6 - attack.low}` : `${attack.stroke === 'down' ? 'Down' : 'Up'} stroke, strings ${6 - attack.low} to ${6 - attack.high}`;
                return mark;
            }));
        }
        const phase = tick % ticksPerBar / ticksPerBar;
        moveCards(phase, !!data && (bar + 1 < data.bars || jumpBar !== null), slideShift(now) + dragShift());
        $('strum-track').style.setProperty('--phase', phase);
        const counting = !state.armed && !state.stale;
        for (const [index, mark] of [...$('attacks').children].entries()) {
            const at = patterns[rhythmIndex][index].offset / 400;
            mark.classList.toggle('active', counting && Math.abs(at - phase) < .035);
            mark.classList.toggle('past', counting && at < phase - .035);
        }
        const half = Math.floor(phase * state.beats * 2);
        for (const [index, mark] of [...$('count').children].entries()) {
            const number = index % 2 === 0;
            const here = number ? Math.floor(half / 2) === index / 2 : half === index;
            mark.classList.toggle('lit', counting && here);
            mark.classList.toggle('past', counting && (number ? index / 2 < Math.floor(half / 2) : index < half));
        }
        $('telemetry').textContent = remote ? `Generation ${sample.generation} · status read ${Math.round(sample.roundTrip)} ms. Beat interpolation uses device samples.` : 'Browser preview uses the shared musical transport.';
    } else {
        view = null;
    }
    requestAnimationFrame(frame);
}

run(async () => {
    const [html, source] = await Promise.all([
        fetch('../').then(response => {if(!response.ok) throw new Error('Cannot load song engine'); return response.text();}),
        fetch('../chord-practice.md').then(response => {if(!response.ok) throw new Error('Cannot load practice song'); return response.text();}),
    ]);
    const modulePath = html.match(/from\s*['"]([^'"]*gallery-web[^'"]*\.js)['"]/)?.[1];
    const wasmPath = html.match(/module_or_path:\s*['"]([^'"]+\.wasm)['"]/)?.[1];
    if (!modulePath || !wasmPath) throw new Error('Song engine unavailable. Build the Web gallery with Trunk.');
    bindings = await import(new URL(modulePath, new URL('../', location.href)).href);
    await bindings.default({ module_or_path: new URL(wasmPath, new URL('../', location.href)).href });
    patterns = JSON.parse(bindings.chord_patterns_json());
    engineReady = true;
    if (!$('source').value) $('source').value = savedDraft() ?? source;
    parseEditor(true);
    if (!preview) editor(true);
    refreshConnection();
});
refreshConnection();
document.addEventListener('visibilitychange', () => deviceSync.setVisible(!document.hidden));
requestAnimationFrame(frame);
window.addEventListener('pagehide', () => {
    saveDraft(); deviceSync.clear(); link.close();
    sample = null; deviceData = null; deviceGeneration = undefined; previewMode = true;
});
window.addEventListener('pageshow', () => { deviceSync.setVisible(!document.hidden); refreshConnection(); });
