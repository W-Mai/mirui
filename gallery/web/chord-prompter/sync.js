const PERIOD = 5000;
const STALE = 15000;
const TICKS_PER_BEAT = 960;
const rate = sample => sample.bpm * TICKS_PER_BEAT / 60000000;

// Device milliseconds wrap, while host sample times remain monotonic.
function startTime(sample, startMillis) {
    return sample.hostMillis + ((startMillis - sample.millis) | 0);
}

export function projectSample(sample, now, startMillis = null, endTick = Infinity) {
    if (!sample) return null;
    const age = Math.max(0, now - sample.hostMillis);
    const start = sample.armed && startMillis !== null ? startTime(sample, startMillis) : sample.hostMillis;
    const armed = sample.armed && (startMillis === null || now < start);
    const tick = sample.tick + (sample.playing && !armed ? Math.max(0, now - start) * rate(sample) : 0);
    const ended = Number.isFinite(endTick) && tick >= endTick;
    return {...sample, tick: Math.min(tick, endTick - 1), playing: sample.playing && !ended,
        armed: armed && !ended, pending: sample.pending && !ended, stale: sample.playing && age >= STALE};
}

// One timer and one coalesced read loop share ChordLink's existing GATT queue.
export class DeviceSync {
    constructor(link, {onSample = () => {}, onError = () => {}, now = () => performance.now(),
        setTimer = setTimeout, clearTimer = clearTimeout, visible = true} = {}) {
        Object.assign(this, {link, onSample, onError, now, setTimer, clearTimer, visible});
        this.active = false;
        this.blocked = false;
        this.timer = null;
        this.sample = null;
        this.startMillis = null;
        this.boundaryTick = null;
        this.revision = 0;
        this.requested = false;
        this.forced = false;
        this.inFlight = null;
        this.endTick = Infinity;
        this.score = null;
        this.retryAt = 0;
    }
    clear() {
        this.cancelTimer();
        this.revision++;
        this.sample = null;
        this.startMillis = this.boundaryTick = null;
        this.requested = false;
        this.forced = false;
        this.inFlight = null;
        this.active = false;
        this.endTick = Infinity;
        this.score = null;
        this.retryAt = 0;
    }
    cancelTimer() {
        if (this.timer !== null) this.clearTimer(this.timer);
        this.timer = null;
    }
    get ready() { return this.active && this.visible && !this.blocked && this.link.connected; }
    setActive(active) {
        if (active === this.active) return;
        this.active = active;
        this.schedule();
    }
    setVisible(visible) {
        if (visible === this.visible) return;
        this.visible = visible;
        this.schedule();
        if (visible && this.ready) this.refresh().catch(this.onError);
    }
    setBlocked(blocked) {
        this.blocked = blocked;
        this.schedule();
    }
    setScore(score) {
        if (score === this.score) return;
        this.score = score;
        this.endTick = score ? score.bars * score.beats * TICKS_PER_BEAT : Infinity;
        this.schedule();
    }
    accept(sample) {
        if (this.sample?.generation !== sample.generation) {
            this.startMillis = this.boundaryTick = null;
        }
        if (!sample.armed) this.startMillis = null;
        if (!sample.pending || (this.boundaryTick !== null && sample.tick >= this.boundaryTick)) this.boundaryTick = null;
        this.sample = sample;
        this.retryAt = 0;
        this.onSample(sample);
        this.schedule();
    }
    noteControl(receipt) {
        if (!this.sample || receipt.generation !== this.sample.generation) return;
        if (receipt.opcode === 1) this.startMillis = receipt.startMillis;
        if (receipt.opcode === 3) this.startMillis = this.boundaryTick = null;
        if (receipt.opcode >= 4 && receipt.opcode <= 6) {
            this.boundaryTick = Math.min(this.boundaryTick ?? Infinity, receipt.tick);
        }
    }
    state(now = this.now()) {
        return projectSample(this.sample, now, this.startMillis, this.endTick);
    }
    schedule() {
        this.cancelTimer();
        if (!this.ready || this.inFlight || !this.sample) return;
        const sample = this.sample, now = this.now(), state = this.state(now);
        if (!state.playing) return;
        let due = sample.hostMillis + sample.roundTrip / 2 + PERIOD;
        const guard = sample.roundTrip / 2 + 1 / rate(sample) + 2;
        if (sample.armed) {
            due = Math.min(due, this.startMillis === null ? now + 250 : startTime(sample, this.startMillis) + guard);
        } else if (sample.pending) {
            const target = this.boundaryTick ?? (Math.floor(sample.tick / TICKS_PER_BEAT) + 1) * TICKS_PER_BEAT;
            due = Math.min(due, sample.hostMillis + (target - sample.tick) / rate(sample) + guard);
        }
        const section = this.score?.sections.find(section => section.tick > sample.tick);
        if (section && !sample.armed) {
            due = Math.min(due, sample.hostMillis + (section.tick - sample.tick) / rate(sample) + guard);
        }
        this.timer = this.setTimer(() => {
            this.timer = null;
            this.refresh().catch(this.onError);
        }, Math.max(20, Math.max(due, this.retryAt) - now));
    }
    refresh({allowBlocked = false} = {}) {
        if (!this.active || !this.visible || !this.link.connected || (this.blocked && !allowBlocked)) return Promise.resolve(null);
        this.cancelTimer();
        this.requested = true;
        this.forced ||= allowBlocked;
        if (this.inFlight) return this.inFlight;
        const revision = this.revision, epoch = this.link.epoch;
        const current = () => revision === this.revision && epoch === this.link.epoch;
        const request = Promise.resolve().then(async () => {
            let latest = null;
            while (current() && this.requested) {
                const forced = this.forced;
                this.requested = false;
                this.forced = false;
                const allowed = () => this.active && this.visible && this.link.connected && (!this.blocked || forced);
                while (this.link.busy && current() && allowed()) await this.link.tail;
                if (!current()) break;
                if (!allowed()) continue;
                const sample = await this.link.sample();
                if (!current()) break;
                this.accept(sample);
                latest = sample;
            }
            return latest;
        }).catch(error => {
            if (current()) { this.retryAt = this.now() + PERIOD; throw error; }
            return null;
        }).finally(() => {
            if (this.inFlight === request) {
                this.inFlight = null;
                this.schedule();
            }
        });
        this.inFlight = request;
        return request;
    }
}
