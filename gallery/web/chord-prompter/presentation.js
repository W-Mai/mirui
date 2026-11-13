// Consume canonical score metadata produced by the shared Rust parser.
export const TICKS_PER_BEAT = 960;

// A bar's slots, each with the chord and the lyric cut into `[text,
// accented]` pairs by the parser, and its summary as text. `index` is the
// slot's place in the whole song. Data without pairs, from before the
// parser sent them, shows its lyric with no accent.
export function barContent(data, bar) {
    if (!data || bar < 0 || bar >= data.bars) return null;
    const start = bar * data.beats * TICKS_PER_BEAT, end = start + data.beats * TICKS_PER_BEAT;
    const slots = [], lyrics = [];
    for (const [index, slot] of data.slots.entries()) {
        if (slot.tick < start || slot.tick >= end) continue;
        slots.push({index, chord: slot.chord, segments: slot.segments ?? [[slot.lyric, false]]});
        lyrics.push(slot.lyric);
    }
    return {
        number: bar + 1,
        chords: slots.map(slot => slot.chord).filter(Boolean).join(' · '),
        lyric: lyrics.join('').trim(),
        slots,
    };
}

// Bars where sections start, and the name of the section holding a bar.
export function sectionStarts(data) {
    return data ? data.sections.map(section => section.tick / (data.beats * TICKS_PER_BEAT)) : [0];
}
export function sectionAt(data, bar) {
    const start = data ? bar * data.beats * TICKS_PER_BEAT : 0;
    return data?.sections.findLast(section => section.tick <= start)?.name ?? '';
}

// Picking a bar. While playing it becomes the bar after the current one, at
// the next bar line; paused or counting in, the position moves there now.
// Returns the bar, 'cancel' to drop a queued jump, or null for no change.
export function choose(at, target) {
    const {bar, offset = 0, playing = false, jump = null} = at;
    if (!playing) return target === bar && offset === 0 ? null : target;
    if (target === (jump ?? bar + 1)) return null;
    return target === bar + 1 ? 'cancel' : target;
}

// Steps by bars or sections: from the bar playback goes to next while
// playing, and from the position while paused, where a step back first
// returns to the start of the current bar or section.
export function steer(at, delta, bySection = false) {
    const {bar, bars, offset = 0, playing = false, jump = null, starts = [0]} = at;
    const from = playing ? jump ?? bar + 1 : bar;
    const behind = start => playing ? start < from : start < bar || (start === bar && offset > 0);
    let target = bySection
        ? delta > 0 ? starts.find(start => start > from) : starts.findLast(behind)
        : delta < 0 && !playing && offset > 0 ? bar : from + delta;
    if (target === undefined) return null;
    target = Math.max(0, Math.min(playing && bar + 1 === bars ? bars : bars - 1, target));
    if (!playing && delta > 0 && target <= bar) return null;
    return choose(at, target);
}

// The played bars behind the current one, most recent first. Playback that
// moved on by itself or through a queued jump keeps the bar it left; a seek,
// stop or new song starts over from the written order.
export function follow(trail, from, to, playing, jumpBar = null) {
    if (from === to) return trail;
    if (playing && from !== null && (to === from + 1 || to === jumpBar)) return [from, ...trail].slice(0, 3);
    return [to - 1, to - 2, to - 3];
}

// The bar a strip card shows: played bars behind the current one, and ahead
// of it the bars playback reaches, through a queued jump `ahead` cards away.
export function stripBar({bar, trail, jumpBar = null, ahead = 1}, offset) {
    if (offset < 0) return trail[-offset - 1] ?? bar + offset;
    if (offset === 0 || jumpBar === null || offset < ahead) return bar + offset;
    return jumpBar + offset - ahead;
}
