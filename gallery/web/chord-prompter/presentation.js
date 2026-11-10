// Consume canonical score metadata produced by the shared Rust parser.
export function barContent(data, bar) {
    if (!data || bar < 0 || bar >= data.bars) return null;
    const start = bar * data.beats * 960, end = start + data.beats * 960;
    const slots = data.slots.filter(slot => slot.tick >= start && slot.tick < end);
    return {
        number: bar + 1,
        chords: slots.map(slot => slot.chord).filter(Boolean).join(' · '),
        lyric: slots.map(slot => slot.lyric).join('').trim(),
    };
}
