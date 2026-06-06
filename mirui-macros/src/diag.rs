pub(crate) fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0usize; b.len() + 1];
    for i in 1..=a.len() {
        curr[0] = i;
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        core::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

/// `max_dist` cuts off too-distant candidates: a wrong hint is worse than no hint.
pub(crate) fn closest<'a, I>(query: &str, candidates: I, max_dist: usize) -> Option<&'a str>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut best: Option<(usize, &'a str)> = None;
    for cand in candidates {
        let d = levenshtein(query, cand);
        if d <= max_dist && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, cand));
        }
    }
    best.map(|(_, n)| n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levenshtein_identity() {
        assert_eq!(levenshtein("abc", "abc"), 0);
    }

    #[test]
    fn levenshtein_one_substitute() {
        assert_eq!(levenshtein("abc", "axc"), 1);
    }

    #[test]
    fn levenshtein_insert_and_delete() {
        assert_eq!(levenshtein("kitten", "sitting"), 3);
    }

    #[test]
    fn closest_returns_within_threshold() {
        let pool = ["text", "text_color", "bg_color"];
        assert_eq!(closest("txt", pool.iter().copied(), 2), Some("text"));
    }

    #[test]
    fn closest_returns_none_when_too_far() {
        let pool = ["text", "bg_color"];
        assert_eq!(closest("zzzz", pool.iter().copied(), 2), None);
    }

    #[test]
    fn closest_picks_smallest_distance() {
        // "bgcolor" → "bg_color" (1 insert) vs "color" (2 deletes)
        let pool = ["bg_color", "color", "text"];
        assert_eq!(
            closest("bgcolor", pool.iter().copied(), 2),
            Some("bg_color")
        );
    }
}
