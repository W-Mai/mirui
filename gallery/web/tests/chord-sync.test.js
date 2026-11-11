import {expect, test} from 'bun:test';
import {DeviceSync, projectSample} from '../chord-prompter/sync.js';

function fixture(initial = {}) {
    let now = 0, reads = 0, serial = 0;
    let state = {playing: true, pending: false, armed: false, generation: 31, tick: 0, bpm: 96000,
        millis: 0, beats: 4, unit: 4, pattern: 2, ...initial};
    const tasks = new Map(), accepted = [], errors = [];
    const link = {connected: true, busy: false, epoch: 0, tail: Promise.resolve(),
        async sample() { reads++; return {...state, hostMillis: now, roundTrip: 0}; }};
    const sync = new DeviceSync(link, {onSample: s => accepted.push(s), onError: e => errors.push(e), now: () => now,
        setTimer: (fn, delay) => {const id = serial++; tasks.set(id, {fn, at: now + delay}); return id;},
        clearTimer: id => tasks.delete(id)});
    sync.setActive(true); sync.accept({...state, hostMillis: now, roundTrip: 0});
    const flush = async () => { for (let i = 0; i < 12; i++) await Promise.resolve(); };
    const advance = async ms => {
        const end = now + ms;
        while (true) {
            const task = [...tasks].sort((a, b) => a[1].at - b[1].at)[0];
            if (!task || task[1].at > end) break;
            now = task[1].at; tasks.delete(task[0]); task[1].fn(); await flush();
        }
        now = end; await flush();
    };
    return {sync, link, tasks, accepted, errors, advance, flush,
        get reads() {return reads;}, get now() {return now;}, set state(next) {state = {...state, ...next};}};
}

test('steady playback samples every five seconds while local projection keeps moving', async () => {
    const f = fixture();
    await f.advance(4999); expect(f.reads).toBe(0);
    expect(f.sync.state().tick).toBeCloseTo(4999 * 1.536);
    expect(f.sync.state().stale).toBe(false);
    await f.advance(1); expect(f.reads).toBe(1);
    await f.advance(10000); expect(f.reads).toBe(3);
    expect(f.tasks.size).toBe(1);
});

test('stable pause and song completion have no periodic reads', async () => {
    const f = fixture({playing: false});
    await f.advance(60000); expect(f.reads).toBe(0);
    expect(f.sync.state().tick).toBe(0);
    const g = fixture(); g.sync.setScore({bars: 1, beats: 4, sections: []});
    g.state = {playing: false, tick: 3839};
    await g.advance(5000); expect(g.reads).toBe(1);
    expect(g.sync.state()).toMatchObject({playing: false, tick: 3839});
    await g.advance(60000); expect(g.reads).toBe(1);
});

test('hidden, inactive and upload states cancel timers; foreground confirms paused state once', async () => {
    const f = fixture({playing: false});
    f.sync.setVisible(false); await f.advance(60000); expect(f.reads).toBe(0);
    f.sync.setVisible(true); await f.flush(); expect(f.reads).toBe(1);
    f.sync.setActive(false); f.sync.setVisible(false); f.sync.setVisible(true);
    await f.advance(60000); expect(f.reads).toBe(1);
    const g = fixture(); g.sync.setBlocked(true);
    await g.advance(60000); expect(g.reads).toBe(0);
    g.sync.setBlocked(false); await g.sync.refresh(); expect(g.reads).toBe(1);
});

test('confirmed delayed start projects at the acknowledged wrapping device time', async () => {
    const f = fixture({playing: false, millis: 0xffffff00});
    f.sync.noteControl({generation: 31, opcode: 1, startMillis: 0});
    f.sync.accept({playing: true, pending: false, armed: true, generation: 31, tick: 0, bpm: 96000,
        millis: 0xffffff00, hostMillis: 0, roundTrip: 0});
    await f.advance(255); expect(f.sync.state()).toMatchObject({armed: true, tick: 0});
    await f.advance(2); expect(f.sync.state().armed).toBe(false);
    expect(f.sync.state().tick).toBeCloseTo(1.536);
    f.state = {armed: false, playing: true, tick: 8};
    await f.advance(10); expect(f.reads).toBe(1);
    expect(f.tasks.size).toBe(1);
});

test('on-demand playback controls can confirm status during upload without resuming periodic reads', async () => {
    const f = fixture(); f.sync.setBlocked(true);
    await f.sync.refresh(); expect(f.reads).toBe(0);
    f.state = {playing: false}; await f.sync.refresh({allowBlocked: true});
    expect(f.reads).toBe(1); expect(f.sync.state().playing).toBe(false);
    await f.advance(30000); expect(f.reads).toBe(1);
});

test('known control boundaries are confirmed before the periodic deadline', async () => {
    const f = fixture();
    f.sync.noteControl({generation: 31, opcode: 4, tick: 960});
    f.sync.accept({...f.sync.sample, pending: true});
    await f.advance(625); expect(f.reads).toBe(0);
    f.state = {tick: 970, pending: false, bpm: 120000};
    await f.advance(10); expect(f.reads).toBe(1);
    expect(f.sync.state().bpm).toBe(120000);
    await f.advance(4000); expect(f.reads).toBe(1);
});

test('unknown armed and queued transitions use bounded temporary confirmations', async () => {
    const f = fixture({armed: true});
    await f.advance(249); expect(f.reads).toBe(0);
    f.state = {armed: false}; await f.advance(1); expect(f.reads).toBe(1);
    await f.advance(4999); expect(f.reads).toBe(1);
    const g = fixture({pending: true});
    await g.advance(625); expect(g.reads).toBe(0);
    g.state = {tick: 970, pending: false};
    await g.advance(10); expect(g.reads).toBe(1);
    const paused = fixture({playing: false, pending: true});
    await paused.advance(60000); expect(paused.reads).toBe(0);
});

test('canonical section transitions trigger confirmation without polling intervening beats', async () => {
    const f = fixture();
    f.sync.setScore({bars: 4, beats: 4, sections: [{tick: 0}, {tick: 3840}]});
    await f.advance(2500); expect(f.reads).toBe(0);
    f.state = {tick: 3850, pattern: 5};
    await f.advance(10); expect(f.reads).toBe(1);
    expect(f.sync.state().pattern).toBe(5);
    await f.advance(4000); expect(f.reads).toBe(1);
});

test('overlapping refreshes coalesce with one trailing confirmation for later controls', async () => {
    const f = fixture(); let finish;
    const first = new Promise(resolve => {finish = resolve;}); let reads = 0;
    f.link.sample = async () => {reads++; if (reads === 1) await first; return {...f.sync.sample};};
    const request = f.sync.refresh(); await f.flush();
    for (let i = 0; i < 20; i++) expect(f.sync.refresh()).toBe(request);
    finish(); await request;
    expect(reads).toBe(2); expect(f.tasks.size).toBe(1);
});

test('sampling waits for the existing GATT queue and retires old connection results', async () => {
    const f = fixture(); let release;
    f.link.busy = true; f.link.tail = new Promise(resolve => {release = resolve;});
    const pending = f.sync.refresh(); await f.flush(); expect(f.reads).toBe(0);
    f.link.busy = false; release(); await pending; expect(f.reads).toBe(1);
    let finish; f.link.sample = () => new Promise(resolve => {finish = resolve;});
    const old = f.sync.refresh(); await f.flush(); f.link.epoch++; f.sync.clear();
    finish({...f.sync.sample, generation: 99}); await old;
    expect(f.sync.sample).toBeNull(); expect(f.tasks.size).toBe(0);
});

test('failed synchronization backs off instead of retrying an overdue timer repeatedly', async () => {
    const f = fixture(); let reads = 0;
    f.link.sample = async () => {reads++; throw new Error('Read failed');};
    await f.advance(5000); expect(reads).toBe(1); expect(f.errors.length).toBe(1);
    await f.advance(4999); expect(reads).toBe(1);
    await f.advance(1); expect(reads).toBe(2);
});

test('stale samples are marked without the former short extrapolation freeze', () => {
    const sample = {playing: true, armed: false, pending: false, tick: 100, bpm: 120000, hostMillis: 10};
    expect(projectSample(sample, 5010)).toMatchObject({tick: 9700, stale: false});
    expect(projectSample(sample, 15010)).toMatchObject({tick: 28900, stale: true});
});
