import { ChordLink, SERVICE, PATTERNS } from './protocol.js';

const $ = id => document.getElementById(id);
let sample = null, connecting = false, polling = false, beatCount = 0;
const message = (text, error = false) => { $('message').textContent = text; $('message').classList.toggle('error', error); };
const link = new ChordLink(() => { sample = null; refreshConnection(); message('Display disconnected. Playback continues on the board.'); });

function refreshConnection() {
    $('connection').textContent = link.connected ? 'DISPLAY CONNECTED' : 'DISPLAY OFFLINE';
    $('lamp').classList.toggle('online', link.connected);
    $('connect').textContent = connecting ? 'Connecting…' : link.connected ? 'Disconnect display' : 'Connect display ↗';
    $('connect').disabled = connecting;
    for (const button of document.querySelectorAll('[data-device]')) button.disabled = !link.connected || connecting;
}

async function run(operation) {
    try { return await operation(); }
    catch (error) { message(error.message, true); refreshConnection(); }
}

async function poll() {
    if (polling || link.busy || !link.connected || connecting) return;
    polling = true;
    try { sample = await link.sample(); }
    catch (error) { message(error.message, true); }
    finally { polling = false; refreshConnection(); }
}

$('connect').onclick = () => run(async () => {
    if (link.connected) { link.close(); sample = null; refreshConnection(); return; }
    if (!navigator.bluetooth) throw new Error('Web Bluetooth is unavailable here. Open this page in Chrome or Arc on desktop or Android.');
    if (!window.isSecureContext) throw new Error('Use HTTPS or localhost for Bluetooth.');
    connecting = true; refreshConnection();
    try {
        const device = await navigator.bluetooth.requestDevice({ filters: [{ namePrefix: 'MIRUI Chord' }], optionalServices: [SERVICE] });
        sample = await link.open(device);
        message(`Connected to ${device.name ?? 'display'}. The device is the playback clock.`);
    } finally { connecting = false; refreshConnection(); }
});

function command(opcode, arg = 0, boundary = 0) {
    return run(async () => {
        const receipt = await link.command(opcode, arg, boundary);
        message(boundary ? `Accepted for tick ${receipt.tick}. Queued changes are marked on the display.` : 'Device acknowledged.');
        await poll();
    });
}
$('play').onclick = () => command(1, 500);
$('pause').onclick = () => command(2);
$('stop').onclick = () => command(3);
$('tempo-form').onsubmit = event => { event.preventDefault(); command(4, Math.round(Number($('tempo').value) * 1000), 1); };
$('seek-form').onsubmit = event => { event.preventDefault(); command(5, Number($('bar').value) - 1, 2); };
$('rhythm-form').onsubmit = event => { event.preventDefault(); command(6, Number($('rhythm').value), 2); };
for (const [index, name] of PATTERNS.entries()) {
    const option = document.createElement('option'); option.value = String(index); option.textContent = name;
    $('rhythm').append(option);
}

function byteCount() { $('bytes').textContent = `${new TextEncoder().encode($('source').value).length} / 8192 BYTES`; }
$('source').oninput = byteCount;
$('file').onchange = () => run(async () => {
    const file = $('file').files[0];
    if (!file) return;
    if (file.size > 8192) throw new Error('Song sheet exceeds the 8192-byte device limit.');
    $('source').value = new TextDecoder('utf-8', { fatal: true }).decode(await file.arrayBuffer());
    byteCount(); message(`Opened ${file.name}. Send it to validate and activate it on the display.`);
});
$('upload').onclick = () => run(async () => {
    $('progress').value = 0;
    await link.upload($('source').value, ratio => { $('progress').value = ratio; });
    message('Score validated and activated. Press Play to start.');
    await poll();
});

function frame() {
    const age = sample ? performance.now() - sample.hostMillis : Infinity;
    const live = link.connected && sample && age < 1500;
    $('run-state').textContent = !link.connected ? 'WAITING FOR DISPLAY' : !live ? 'WAITING FOR DEVICE STATUS' : sample.armed ? 'START ARMED' : sample.playing ? (sample.pending ? 'PLAYING · CHANGE QUEUED' : 'PLAYING') : 'PAUSED / STOPPED';
    if (live) {
        const tick = sample.tick + (sample.playing && !sample.armed ? Math.max(0, age) * sample.bpm * 960 / 60000000 : 0);
        const beat = Math.floor(tick / 960) % sample.beats;
        if (beatCount !== sample.beats) {
            $('beats').replaceChildren(...Array.from({ length: sample.beats }, () => document.createElement('i')));
            beatCount = sample.beats;
        }
        for (const [index, dot] of [...$('beats').children].entries()) dot.classList.toggle('active', !sample.armed && index === beat);
        $('position').textContent = `BAR ${String(Math.floor(tick / (960 * sample.beats)) + 1).padStart(2, '0')} · ${beat + 1}`;
        $('tempo-live').textContent = String(sample.bpm / 1000);
        $('meter').textContent = `${sample.beats} / ${sample.unit}`;
        $('telemetry').textContent = `${PATTERNS[sample.pattern]} · generation ${sample.generation} · BLE read ${Math.round(sample.roundTrip)} ms round trip. Beat interpolation follows the last device sample.`;
    } else {
        for (const dot of $('beats').children) dot.classList.remove('active');
    }
    requestAnimationFrame(frame);
}

run(async () => {
    const response = await fetch('../chord-practice.md');
    if (!response.ok) throw new Error('Practice song sheet could not be loaded. Open a local .md file.');
    $('source').value = await response.text(); byteCount();
});
refreshConnection();
setInterval(() => { if (!document.hidden) poll(); }, 180);
requestAnimationFrame(frame);
window.addEventListener('pagehide', () => link.close());
