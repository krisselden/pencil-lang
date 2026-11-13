//! What an unknown name might have meant. A name is never read as anything but itself, so a typo
//! stays an error, but the error can say what's close: a defined name the typed one starts or is
//! a slip away from, as when Enter took a half-typed name, or a split into defined names, as when
//! a space went missing.

/// "unknown name `x`", with a suggestion when there's one.
pub fn unknown<'a>(what: &str, name: &str, known: impl IntoIterator<Item = &'a str>) -> String {
    match suggestion(name, known) {
        Some(hint) => format!("unknown {what} `{name}`: did you mean {hint}?"),
        None => format!("unknown {what} `{name}`"),
    }
}

/// The closest names to `unknown` among `known`, as `` `a` `` or `` `a` or `b c` ``.
pub fn suggestion<'a>(unknown: &str, known: impl IntoIterator<Item = &'a str>) -> Option<String> {
    let mut known: Vec<&str> = known.into_iter().filter(|k| *k != unknown).collect();
    known.sort_unstable();
    known.dedup();
    let close = closest(unknown, &known);
    let split = split(unknown, &known).filter(|s| Some(s.as_str()) != close);
    match (close, split) {
        (Some(c), Some(s)) => Some(format!("`{c}` or `{s}`")),
        (Some(c), None) => Some(format!("`{c}`")),
        (None, Some(s)) => Some(format!("`{s}`")),
        (None, None) => None,
    }
}

/// A name that starts with `unknown`, or else the nearest within a typo or two, by edit distance.
fn closest<'a>(unknown: &str, known: &[&'a str]) -> Option<&'a str> {
    let len = unknown.chars().count();
    if len >= 2 {
        let longer = known
            .iter()
            .filter(|k| k.starts_with(unknown))
            .min_by_key(|k| k.chars().count());
        if let Some(k) = longer {
            return Some(k);
        }
    }
    let reach = match len {
        0..=2 => return None,
        3..=5 => 1,
        6..=9 => 2,
        _ => 3,
    };
    known
        .iter()
        .map(|k| (distance(unknown, k), *k))
        .filter(|(d, _)| *d <= reach)
        .min()
        .map(|(_, k)| k)
}

/// Edit distance, counting a swap of two neighbors as one edit.
fn distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut d = vec![vec![0; b.len() + 1]; a.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for j in 0..=b.len() {
        d[0][j] = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            d[i][j] = (d[i - 1][j] + 1).min(d[i][j - 1] + 1).min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
            }
        }
    }
    d[a.len()][b.len()]
}

/// `unknown` as two or more defined names written together, in the fewest pieces.
fn split(unknown: &str, known: &[&str]) -> Option<String> {
    let chars: Vec<char> = unknown.chars().collect();
    // The fewest pieces that cover each prefix, and where the last piece starts.
    let mut best: Vec<Option<(usize, usize)>> = vec![None; chars.len() + 1];
    best[0] = Some((0, 0));
    for end in 1..=chars.len() {
        for start in 0..end {
            let Some((pieces, _)) = best[start] else { continue };
            let piece: String = chars[start..end].iter().collect();
            if known.contains(&piece.as_str())
                && best[end].is_none_or(|(fewest, _)| pieces + 1 < fewest)
            {
                best[end] = Some((pieces + 1, start));
            }
        }
    }
    let (pieces, _) = best[chars.len()]?;
    if pieces < 2 {
        return None;
    }
    let mut words = Vec::new();
    let mut end = chars.len();
    while end > 0 {
        let (_, start) = best[end].unwrap();
        words.push(chars[start..end].iter().collect::<String>());
        end = start;
    }
    words.reverse();
    Some(words.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_half_typed_name_suggests_the_whole_one() {
        let known = ["ToCoords", "Total", "t"];
        assert_eq!(suggestion("ToCo", known), Some("`ToCoords`".into()));
    }

    #[test]
    fn a_typo_suggests_the_name_it_slipped_from() {
        let known = ["radius", "ratio", "theta"];
        assert_eq!(suggestion("raduis", known), Some("`radius`".into()));
        assert_eq!(suggestion("thtea", known), Some("`theta`".into()));
        assert_eq!(suggestion("zz", known), None);
    }

    #[test]
    fn names_run_together_suggest_the_split() {
        let known = ["one", "two", "three", "t"];
        assert_eq!(suggestion("onetwothree", known), Some("`one two three`".into()));
        // Not a split that leaves something undefined, as `t hree` would.
        assert_eq!(suggestion("onetwothre", known), None);
        assert_eq!(
            unknown("name", "onetwo", known),
            "unknown name `onetwo`: did you mean `one two`?"
        );
    }
}
