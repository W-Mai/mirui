import { ChordLink, SERVICE, PATTERNS } from './protocol.js';
import { barContent } from './presentation.js';
import { DeviceSync } from './sync.js';

const $ = id => document.getElementById(id);
const reducedMotion = matchMedia('(prefers-reduced-motion: reduce)');
const mobileLayout = matchMedia('(max-width: 800px)');
const appearanceQuery = matchMedia('(prefers-color-scheme: dark)');
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
const errorText = error => String(error?.message ?? error);
let bindings, patterns = [], preview, previewData, draftData, deviceData, deviceGeneration;
let engineReady = false;
let sample = null, connecting = false, uploading = false, previewMode = true;
let beatCount = 0, rhythmIndex = -1, lyricKey = '', activeSlot = -1, revision = 0, editTimer, scrubbing = false;
const barCards = [...document.querySelectorAll('.bar-card')];
const message = (text = '', error = false) => { $('message').textContent = text; $('message').classList.toggle('error', error); };
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
window.addEventListener('keydown', event => {
    if (event.code === 'Escape' && $('workbench').classList.contains('editor-open')) { editor(false); return; }
    if (event.code !== 'Space' || event.repeat || event.altKey || event.ctrlKey || event.metaKey) return;
    if (event.target.closest('a,input,textarea,select,button,summary')) return;
    event.preventDefault(); $('play').click();
});

function associateDeviceSample(next) {
    sample = next;
    if (deviceGeneration !== next.generation) {
        deviceData = null;
        try {
            const saved = JSON.parse(sessionStorage.getItem('mirui-chord-active') ?? 'null');
            if (saved?.generation === next.generation && saved.device === (link.device?.id ?? '')
                && Array.isArray(saved.data?.sections)) deviceData = saved.data;
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
        preview?.pause(performance.now()); previewMode = false; lyricKey = '';
        deviceSync.accept(next);
        message(deviceData ? '' : 'Display connected · song source is not synced');
    } finally { connecting = false; refreshConnection(); }
});

async function control(opcode, arg = 0, boundary = 0) {
    if (!engineReady || (!preview && !usingDevice())) throw new Error('Load a song first');
    if (usingDevice()) {
        const receipt = await link.command(opcode, arg, boundary);
        deviceSync.noteControl(receipt);
        await poll(true);
    } else {
        const now = performance.now();
        if (opcode === 1) preview.play(now);
        else if (opcode === 2) preview.pause(now);
        else if (opcode === 3) preview.stop();
        else if (opcode === 4) preview.tempo(now, arg);
        else if (opcode === 5) preview.seek(now, arg);
        else if (opcode === 6) preview.rhythm(now, arg);
    }
    message(opcode === 4 ? 'Tempo · next beat' : opcode === 5 ? 'Jump · next bar' : opcode === 6 ? 'Strum · next bar' : '');
}
$('play').onclick = () => run(() => {
    const playing = usingDevice() ? deviceSync.state()?.playing : preview?.playing;
    return control(playing ? 2 : 1, usingDevice() && !playing ? 500 : 0);
});
$('stop').onclick = () => run(() => control(3));
$('tempo-form').onsubmit = event => { event.preventDefault(); $('tempo').blur(); };
$('tempo').onchange = () => { if ($('tempo').validity.valid) run(() => control(4, Math.round(Number($('tempo').value) * 1000), 1)); };
$('seek-form').onsubmit = event => { event.preventDefault(); run(() => control(5, Number($('bar').value) - 1, 2)); };
$('rhythm-form').onsubmit = event => { event.preventDefault(); run(() => control(6, Number($('rhythm').value), 2)); };
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
        $(`${name}-label`).textContent = `Bar ${inspectedBar + offset + 1}`;
        $(`${name}-chord`).textContent = content?.chords ?? '';
        $(`${name}-lyric`).textContent = content ? content.lyric || 'Instrumental' : data ? 'End of song' : 'Song source not synced';
    }
}
const positionRange = $('position-range');
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
positionRange.onfocus = () => showBarPreview(Number(positionRange.value) - 1);
positionRange.onblur = hideBarPreview;
positionRange.onkeydown = event => { if (event.key === 'Escape') hideBarPreview(); };
$('position-range').oninput = () => { scrubbing = true; $('position').textContent = `Bar ${$('position-range').value}`; showBarPreview(Number(positionRange.value) - 1); };
$('position-range').onchange = () => { run(() => control(5, Number($('position-range').value) - 1, 2)); scrubbing = false; };
for (const [index, name] of PATTERNS.entries()) {
    const option = document.createElement('option'); option.value = String(index); option.textContent = name; $('rhythm').append(option);
}

function byteCount() { $('bytes').textContent = `${new TextEncoder().encode($('source').value).length} / 8192 bytes`; }
function parseEditor(activate = false) {
    byteCount();
    if (!engineReady) return;
    let candidate;
    try {
        candidate = new bindings.ChordPreview($('source').value);
        draftData = JSON.parse(candidate.score_json());
        $('validation').textContent = `${draftData.bars} bars`; $('validation').classList.remove('error');
        if (activate) {
            preview?.free(); preview = candidate; candidate = null;
            previewData = draftData; previewMode = true; revision++; lyricKey = ''; message('');
        }
    } catch (error) {
        draftData = null;
        $('validation').textContent = errorText(error); $('validation').classList.add('error');
        if (activate) message(errorText(error), true);
    } finally { candidate?.free(); refreshConnection(); }
}
$('source').oninput = () => { byteCount(); clearTimeout(editTimer); editTimer = setTimeout(() => parseEditor(), 250); };
$('preview').onclick = () => parseEditor(true);
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
        preview?.pause(performance.now()); lyricKey = '';
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

function slotAt(data, tick) {
    let lo = 0, hi = data.slots.length;
    while (lo < hi) { const mid = (lo + hi) >>> 1; if (data.slots[mid].tick <= tick) lo = mid + 1; else hi = mid; }
    return Math.max(0, lo - 1);
}
function updateWords(data, tick, identity) {
    if (!data) {
        if (lyricKey !== 'unknown') {
            lyricKey = 'unknown'; $('chord').textContent = '—'; $('lyric').textContent = 'Song not synced';
            $('next-chord').textContent = $('next-lyric').textContent = $('section-name').textContent = '';
            for (const card of barCards) {
                card.hidden = Number(card.dataset.offset) !== 0;
                card.querySelector('.bar-number').textContent = '';
            }
            $('song-title').textContent = 'Chord Prompter';
        }
        return;
    }
    const index = slotAt(data, tick), bar = Math.floor(tick / (data.beats * 960)), key = `${identity}:${bar}`;
    if (key === lyricKey) { markActiveSlot(index); return; }
    const current = data.slots[index];
    lyricKey = key;
    for (const card of barCards) {
        const content = barContent(data, bar + Number(card.dataset.offset));
        card.hidden = !content;
        card.querySelector('.bar-number').textContent = content ? String(content.number).padStart(2, '0') : '';
        card.querySelector('.chord').textContent = content?.chords ?? '';
        card.querySelector('.lyric').textContent = content?.lyric || (content ? 'Instrumental' : '');
    }
    const chords = [], lyrics = [];
    for (const [slotIndex, slot] of data.slots.entries()) {
        if (Math.floor(slot.tick / (data.beats * 960)) !== bar) continue;
        const chord = document.createElement('span'), lyric = document.createElement('span');
        chord.dataset.slot = lyric.dataset.slot = String(slotIndex);
        chord.textContent = slot.chord; lyric.textContent = slot.lyric;
        if (chords.length) chords.push(document.createTextNode(' · '));
        chords.push(chord); lyrics.push(lyric);
    }
    $('chord').replaceChildren(...chords);
    if (lyrics.some(span => span.textContent)) $('lyric').replaceChildren(...lyrics);
    activeSlot = -1; markActiveSlot(index);
    $('section-name').textContent = current.section; $('song-title').textContent = data.title;
    document.title = `${data.title} · Chord Prompter`;
}
function markActiveSlot(index) {
    if (activeSlot === index) return;
    activeSlot = index;
    for (const span of $('current-card').querySelectorAll('[data-slot]')) span.classList.toggle('active-slot', Number(span.dataset.slot) === index);
}
function moveCards(phase, hasNext) {
    const travel = reducedMotion.matches || !hasNext ? 0 : phase;
    for (const card of barCards) {
        const relative = Number(card.dataset.offset) - travel;
        const emphasis = bindings.chord_center_emphasis(Math.round(relative * 256)) / 256;
        card.style.transform = `translate(calc(${relative} * var(--bar-step) - 50%), -50%) scale(${1 + emphasis * .42})`;
        card.style.opacity = String((relative < -.5 ? .45 : .75) + emphasis * .25);
    }
}
function frame() {
    refreshBarPreview();
    const now = performance.now();
    const remote = usingDevice();
    const data = remote ? deviceData : previewData;
    let state;
    if (remote && sample) {
        state = deviceSync.state(now);
    } else if (!remote && preview) {
        state = { tick: preview.tick(now), bpm: preview.bpm, playing: preview.playing, beats: previewData.beats, unit: previewData.unit, pattern: preview.pattern, pending: preview.pending, armed: false, stale: false };
    }
    if (state) {
        const maxTick = data ? data.bars * data.beats * 960 - 1 : Infinity;
        const tick = Math.min(state.tick, maxTick), beat = Math.floor(tick / 960) % state.beats, bar = Math.floor(tick / (960 * state.beats));
        if (beatCount !== state.beats) {
            $('beats').replaceChildren(...Array.from({length: state.beats}, (_, index) => {const dot = document.createElement('i'); dot.textContent=String(index+1); return dot;})); beatCount = state.beats;
        }
        for (const [index, dot] of [...$('beats').children].entries()) dot.classList.toggle('active', !state.armed && !state.stale && index === beat);
        $('meter').textContent = `${state.beats} / ${state.unit}`;
        $('play').setAttribute('aria-pressed', String(state.playing));
        $('play').setAttribute('aria-label', state.playing ? 'Pause' : 'Play');
        $('play').title = state.playing ? 'Pause' : 'Play';
        if (!scrubbing) { $('position').textContent = `Bar ${bar + 1} / ${data?.bars ?? '—'}${state.pending ? ' · queued' : ''}`; $('position-range').max = String(data?.bars ?? 128); $('position-range').value = String(bar + 1); }
        if (document.activeElement !== $('tempo') && !state.pending) $('tempo').value = String(state.bpm / 1000);
        updateWords(data, tick, remote ? `device-${deviceGeneration}` : `preview-${revision}`);
        if (rhythmIndex !== state.pattern) {
            rhythmIndex = state.pattern;
            $('attacks').replaceChildren(...(patterns[rhythmIndex] ?? []).map(attack => {
                const mark = document.createElement('span'); mark.className = `attack ${attack.stroke}`;
                mark.textContent = attack.stroke === 'down' ? '↓' : attack.stroke === 'up' ? '↑' : '●';
                mark.style.left = `${attack.offset / 4}%`; mark.title = attack.stroke === 'pick' ? `String ${6 - attack.low}` : `${attack.stroke} · strings ${6 - attack.low}–${6 - attack.high}`;
                return mark;
            }));
        }
        const phase = tick % (state.beats * 960) / (state.beats * 960);
        moveCards(phase, !!data && bar + 1 < data.bars);
        $('strum-track').style.setProperty('--phase', phase);
        for (const [index, mark] of [...$('attacks').children].entries()) mark.classList.toggle('active', !state.armed && !state.stale && Math.abs(patterns[rhythmIndex][index].offset / 400 - phase) < .035);
        $('telemetry').textContent = remote ? `Generation ${sample.generation} · status read ${Math.round(sample.roundTrip)} ms. Beat interpolation uses device samples.` : 'Browser preview uses the shared musical transport.';
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
