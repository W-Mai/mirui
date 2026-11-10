import { describe, expect, test } from 'bun:test';
import { ChordLink, DeviceError, checksum, statusPacket } from '../chord-prompter/protocol.js';

function mock() {
    const writes = [], received = new Map();
    let seq = 0, generation = 31, receipt = new Uint8Array(20), active = 0, peak = 0, loseRead = false, failOpcode = 0;
    const bytes = new Uint8Array(20), status = new DataView(bytes.buffer);
    bytes[0] = 1; bytes[1] = 0x21; bytes[2] = 4; bytes[3] = 4;
    status.setUint32(4, generation, true); status.setUint32(12, 96000, true);
    async function serial(operation) {
        peak = Math.max(peak, ++active);
        await Promise.resolve();
        try { return operation(); } finally { active--; }
    }
    const control = {
        readValue: () => serial(() => {
            if (loseRead) { loseRead = false; throw new Error('Lost response'); }
            return new DataView(receipt.slice().buffer);
        }),
        writeValueWithResponse: packet => serial(() => {
            const p = new Uint8Array(packet), v = new DataView(p.buffer);
            writes.push(p);
            const requestSeq = v.getUint16(1, true);
            const prior = received.get(requestSeq);
            if (prior) { expect([...p]).toEqual([...prior]); return; }
            expect(requestSeq).toBe(seq);
            seq = (seq + 1) & 65535;
            received.set(requestSeq, p);
            receipt = new Uint8Array(20);
            const r = new DataView(receipt.buffer);
            receipt[0] = p[0] === failOpcode ? 9 : 0; receipt[1] = p[0];
            r.setUint16(2, requestSeq, true);
            if (p[0] === 18 && !receipt[0]) generation++;
            r.setUint32(4, generation, true); r.setUint16(16, seq, true);
        }),
    };
    let disconnectHandler;
    const device = {
        addEventListener: (_, fn) => { disconnectHandler = fn; },
        removeEventListener: () => {},
        gatt: {
            connected: false,
            disconnect() { this.connected = false; },
            async connect() { this.connected = true; return this; },
            async getPrimaryService() { return { getCharacteristic: async uuid => uuid.includes('0003') ? { readValue: () => serial(() => new DataView(bytes.buffer)) } : control }; },
        },
    };
    return { device, writes, get peak() { return peak; }, lose: () => { loseRead = true; }, fail: op => { failOpcode = op; }, disconnect: () => { device.gatt.connected = false; disconnectHandler(); } };
}

describe('bounded device protocol', () => {
    test('status rejects malformed packets and decodes wrapping device time', () => {
        expect(() => statusPacket(new DataView(new ArrayBuffer(19)))).toThrow();
        const bytes = new Uint8Array(20), v = new DataView(bytes.buffer);
        bytes[0] = 1; bytes[1] = 0x25; bytes[2] = 6; bytes[3] = 8;
        v.setUint32(12, 96500, true); v.setUint32(16, 0xffffffff, true);
        expect(statusPacket(v)).toMatchObject({ armed: true, playing: true, pattern: 2, beats: 6, unit: 8, millis: 0xffffffff });
        expect(checksum(new TextEncoder().encode('hello'))).toBe(0x4f9f2cab);
    });
    test('all GATT requests are serial and a lost receipt retries identical bytes', async () => {
        const m = mock(), link = new ChordLink(); await link.open(m.device);
        m.lose();
        await Promise.all([link.command(1, 500), link.sample(), link.command(4, 120000, 1)]);
        expect(m.peak).toBe(1);
        expect(m.writes.length).toBe(3);
        expect([...m.writes[0]]).toEqual([...m.writes[1]]);
        expect(m.writes[2][7]).toBe(1);
        expect(link.seq).toBe(2);
    });
    test('upload preserves UTF-8 across chunk boundaries and activates once', async () => {
        const m = mock(), link = new ChordLink(); await link.open(m.device);
        const text = '1234567890123跟着节拍'.repeat(10), bytes = new TextEncoder().encode(text), progress = [];
        await link.upload(text, value => progress.push(value));
        const chunks = m.writes.filter(packet => packet[0] === 17), decoded = [];
        for (const [index, chunk] of chunks.entries()) {
            expect(new DataView(chunk.buffer).getUint16(3, true)).toBe(index * 14);
            decoded.push(...chunk.slice(6, 6 + chunk[5]));
        }
        expect(decoded).toEqual([...bytes]);
        expect(progress.at(-1)).toBe(1);
        expect(m.writes[0][0]).toBe(16); expect(m.writes.at(-1)[0]).toBe(18);
        expect(new DataView(m.writes[0].buffer).getUint32(12, true)).toBe(checksum(bytes));
        expect(link.generation).toBe(32);
        await expect(link.upload('x'.repeat(8193))).rejects.toThrow('8192');
    });
    test('domain errors consume sequence without retrying or disconnecting', async () => {
        const m = mock(), link = new ChordLink(); await link.open(m.device); m.fail(18);
        await expect(link.upload('[🦀] @ 1')).rejects.toBeInstanceOf(DeviceError);
        expect(m.writes.filter(p => p[0] === 18).length).toBe(1);
        expect(link.generation).toBe(31);
        expect(link.connected).toBe(true);
        await link.command(3);
    });
    test('uploads yield the same ordered stream to playback controls and status', async () => {
        const m = mock(), link = new ChordLink(); await link.open(m.device);
        const upload = link.upload('x'.repeat(200));
        await expect(link.upload('another song')).rejects.toThrow('already running');
        await Promise.all([upload, link.command(3), link.sample()]);
        const opcodes = m.writes.map(packet => packet[0]);
        expect(opcodes[0]).toBe(16);
        expect(opcodes.indexOf(3)).toBeGreaterThan(0);
        expect(opcodes.indexOf(3)).toBeLessThan(opcodes.indexOf(18));
        expect(m.peak).toBe(1);
    });
    test('queued controls keep the score generation selected by their caller', async () => {
        const m = mock(), link = new ChordLink(); await link.open(m.device);
        let release;
        const gate = new Promise(resolve => { release = resolve; });
        const earlier = link.enqueue(async () => { await gate; link.generation = 45; });
        const control = link.command(5, 1, 2);
        release();
        await Promise.all([earlier, control]);
        expect(new DataView(m.writes[0].buffer).getUint32(3, true)).toBe(31);
    });
    test('queued work is bounded and retired at disconnect', async () => {
        const m = mock(), link = new ChordLink(); await link.open(m.device);
        const queued = Array.from({ length: 8 }, () => link.command(3));
        const rejected = link.command(3);
        m.disconnect();
        await expect(rejected).rejects.toThrow('busy');
        const results = await Promise.allSettled(queued);
        expect(results.every(result => result.status === 'rejected')).toBe(true);
        expect(link.connected).toBe(false);
        expect(m.writes.length).toBe(0);
        await link.open(m.device);
        expect(link.seq).toBe(0);
    });
});
