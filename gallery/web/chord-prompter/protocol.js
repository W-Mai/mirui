export const SERVICE = '7e4b0001-8c20-4b6d-a52b-9f7021cf4420';
const uuid = n => SERVICE.replace('0001', n);
export const PATTERNS = ['whole', 'quarter', 'pop', 'poprock', 'ballad', 'folk', 'rock', 'punk', 'reggae', 'shuffle', 'country', 'arpeggio'];
const errors = ['OK', 'Request out of order; reconnect', 'Score changed; read device state', 'Invalid control packet', 'Device capacity exceeded', 'Incomplete or out-of-order upload', 'Upload checksum mismatch', 'Invalid song sheet', 'Control rejected or queue full', 'Text contains characters outside the bundled font'];

export function checksum(bytes) {
    let hash = 0x811c9dc5;
    for (const byte of bytes) hash = Math.imul(hash ^ byte, 0x01000193) >>> 0;
    return hash;
}

function view(data) {
    if (data.byteLength !== 20) throw new Error('Expected a 20-byte device packet');
    return new DataView(data.buffer, data.byteOffset, data.byteLength);
}

export function statusPacket(data) {
    const v = view(data);
    if (v.getUint8(0) !== 1) throw new Error('Unsupported device protocol');
    const flags = v.getUint8(1);
    const beats = v.getUint8(2), bpm = v.getUint32(12, true);
    if (!beats || !bpm || flags >> 4 >= PATTERNS.length) throw new Error('Invalid device timing');
    return { playing: !!(flags & 1), pending: !!(flags & 2), armed: !!(flags & 4), uploading: !!(flags & 8),
        pattern: flags >> 4, beats, unit: v.getUint8(3), generation: v.getUint32(4, true),
        tick: v.getUint32(8, true), bpm, millis: v.getUint32(16, true) };
}

export function receiptPacket(data) {
    const v = view(data);
    return { error: v.getUint8(0), opcode: v.getUint8(1), seq: v.getUint16(2, true),
        generation: v.getUint32(4, true), tick: v.getUint32(8, true), startMillis: v.getUint32(12, true), next: v.getUint16(16, true) };
}

export class DeviceError extends Error {
    constructor(receipt) {
        super(errors[receipt.error] ?? `Unknown device result ${receipt.error}`);
        this.receipt = receipt;
    }
}

// Control, upload and status share a single bounded GATT operation stream.
export class ChordLink {
    constructor(onDisconnect = () => {}) {
        this.onDisconnect = onDisconnect;
        this.epoch = 0;
        this.count = 0;
        this.tail = Promise.resolve();
    }
    get connected() { return !!this.device?.gatt?.connected && !!this.control; }
    get busy() { return this.count !== 0; }
    close() {
        const device = this.device;
        device?.removeEventListener('gattserverdisconnected', this.disconnected);
        this.epoch++;
        this.device = this.control = this.status = this.uploadChar = null;
        this.count = 0;
        this.tail = Promise.resolve();
        device?.gatt?.disconnect();
    }
    async open(device) {
        this.close();
        const epoch = this.epoch;
        this.device = device;
        this.disconnected = () => { this.close(); this.onDisconnect(); };
        device.addEventListener('gattserverdisconnected', this.disconnected);
        try {
            const server = await this.io(device.gatt.connect());
            const service = await this.io(server.getPrimaryService(SERVICE));
            const control = await this.io(service.getCharacteristic(uuid('0002')));
            const status = await this.io(service.getCharacteristic(uuid('0003')));
            const upload = await this.io(service.getCharacteristic(uuid('0004')));
            if (epoch !== this.epoch) throw new Error('Connection replaced');
            this.control = control; this.status = status; this.uploadChar = upload;
            const sample = await this.sample();
            const receipt = await this.enqueue(async () => receiptPacket(await this.io(this.control.readValue())));
            this.seq = receipt.next;
            return sample;
        } catch (error) {
            if (epoch === this.epoch) this.close();
            throw error;
        }
    }
    async io(operation) {
        const epoch = this.epoch;
        let timer;
        try {
            const result = await Promise.race([operation, new Promise((_, reject) => {
                timer = setTimeout(() => {
                    if (epoch === this.epoch) { this.close(); this.onDisconnect(); }
                    reject(new Error('Bluetooth timed out; reconnect before retrying'));
                }, 12000);
            })]);
            if (epoch !== this.epoch) throw new Error('Connection closed');
            return result;
        } finally { clearTimeout(timer); }
    }
    enqueue(operation) {
        if (!this.connected) return Promise.reject(new Error('Connect the display first'));
        if (this.count >= 8) return Promise.reject(new Error('Controls busy; wait for a receipt'));
        const epoch = this.epoch;
        this.count++;
        const result = this.tail.then(() => {
            if (epoch !== this.epoch || !this.connected) throw new Error('Connection closed');
            return operation();
        });
        this.tail = result.catch(() => {});
        return result.finally(() => { if (epoch === this.epoch) this.count--; });
    }
    sample() {
        return this.enqueue(async () => {
            const before = performance.now();
            const sample = statusPacket(await this.io(this.status.readValue()));
            const after = performance.now();
            sample.hostMillis = (before + after) / 2;
            sample.roundTrip = after - before;
            this.generation = sample.generation;
            return sample;
        });
    }
    packet(opcode, arg = 0, boundary = 0, target = 0) {
        const bytes = new Uint8Array(20), v = new DataView(bytes.buffer);
        bytes[0] = opcode; v.setUint16(1, this.seq, true); v.setUint32(3, this.generation, true);
        bytes[7] = boundary; v.setUint32(8, arg, true); v.setUint32(12, target, true);
        return bytes;
    }
    async exchange(bytes, characteristic = this.control) {
        const requestSeq = new DataView(bytes.buffer).getUint16(1, true);
        const epoch = this.epoch;
        let reply;
        for (let attempt = 0; attempt < 2; attempt++) {
            try {
                await this.io(characteristic.writeValueWithResponse(bytes));
                reply = receiptPacket(await this.io(this.control.readValue()));
                if (reply.seq !== requestSeq || reply.opcode !== bytes[0]) throw new Error('Receipt identity mismatch');
                break;
            } catch (error) {
                if (attempt || !this.connected || epoch !== this.epoch) {
                    if (epoch === this.epoch) { this.close(); this.onDisconnect(); }
                    throw error;
                }
                // Only retry the exact packet: the board deduplicates its receipt.
            }
        }
        this.seq = reply.next;
        this.generation = reply.generation;
        if (reply.error === 1 || reply.error === 2) {
            this.close();
            this.onDisconnect();
        }
        if (reply.error) throw new DeviceError(reply);
        return reply;
    }
    command(opcode, arg = 0, boundary = 0, target = 0) {
        return this.enqueue(() => this.exchange(this.packet(opcode, arg, boundary, target)));
    }
    upload(source, progress = () => {}) {
        const bytes = new TextEncoder().encode(source);
        if (!bytes.length || bytes.length > 8192) return Promise.reject(new Error('Song sheet must contain 1–8192 UTF-8 bytes'));
        return this.enqueue(async () => {
            await this.exchange(this.packet(16, bytes.length, 0, checksum(bytes)));
            for (let offset = 0; offset < bytes.length; offset += 14) {
                const chunk = bytes.subarray(offset, offset + 14), packet = new Uint8Array(20), v = new DataView(packet.buffer);
                packet[0] = 17; v.setUint16(1, this.seq, true); v.setUint16(3, offset, true);
                packet[5] = chunk.length; packet.set(chunk, 6);
                await this.exchange(packet, this.uploadChar);
                progress((offset + chunk.length) / bytes.length);
            }
            return this.exchange(this.packet(18));
        });
    }
}
