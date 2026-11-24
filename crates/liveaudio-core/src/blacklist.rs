//! Hallucination blacklist filter: whole-word phrases matched only at utterance edges.

/// Outcome of filtering one utterance against the blacklist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlacklistVerdict {
    /// No phrase sits at an edge: publish the text untouched.
    Keep,
    /// Edge phrases were stripped: publish this remainder.
    Trim(String),
    /// The whole utterance was blacklisted phrases: publish nothing.
    Drop,
}

/// Lowercases and folds common Latin diacritics so accented and plain spellings compare equal.
pub fn fold_diacritics(s: &str) -> String {
    s.chars()
        .flat_map(|c| c.to_lowercase())
        .map(|c| match c {
            'á' | 'à' | 'ä' | 'â' | 'ã' | 'ā' => 'a',
            'é' | 'è' | 'ë' | 'ê' | 'ē' => 'e',
            'í' | 'ì' | 'ï' | 'î' | 'ī' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' | 'õ' | 'ō' => 'o',
            'ú' | 'ù' | 'ü' | 'û' | 'ū' => 'u',
            other => other,
        })
        .collect()
}

/// Whitespace-separated words as `(folded alphanumerics, start byte, end byte)`; pure punctuation is skipped.
fn words(s: &str) -> Vec<(String, usize, usize)> {
    let mut out = Vec::new();
    let mut start = None;
    let mut push = |from: usize, to: usize| {
        let norm: String = fold_diacritics(&s[from..to]).chars().filter(|c| c.is_alphanumeric()).collect();
        if !norm.is_empty() {
            out.push((norm, from, to));
        }
    };
    for (i, c) in s.char_indices() {
        match (c.is_whitespace(), start) {
            (true, Some(from)) => {
                push(from, i);
                start = None;
            }
            (false, None) => start = Some(i),
            _ => {}
        }
    }
    if let Some(from) = start {
        push(from, s.len());
    }
    out
}

/// Phrases of the comma, semicolon or newline separated list, as folded word sequences, longest first.
fn phrases(blacklist_csv: &str) -> Vec<Vec<String>> {
    let quotes = ['"', '\'', '“', '”', '‘', '’', '«', '»'];
    let mut list: Vec<Vec<String>> = blacklist_csv
        .split([',', ';', '\n', '\r'])
        .map(|entry| words(entry.trim().trim_matches(&quotes[..])).into_iter().map(|w| w.0).collect::<Vec<_>>())
        .filter(|p| !p.is_empty())
        .collect();
    list.sort_by_key(|p| std::cmp::Reverse(p.len()));
    list
}

/// Strips blacklisted phrases, as whole words, only at the start or end of `text`.
/// A phrase in the middle of the text never matches.
pub fn filter_blacklisted(text: &str, blacklist_csv: &str) -> BlacklistVerdict {
    let phrases = phrases(blacklist_csv);
    let words = words(text);
    let (mut lo, mut hi) = (0, words.len());
    let matches = |w: &[(String, usize, usize)], p: &Vec<String>| {
        w.len() >= p.len() && w.iter().zip(p).all(|(a, b)| &a.0 == b)
    };
    loop {
        let head = phrases.iter().find(|p| matches(&words[lo..hi], p)).map(|p| p.len());
        if let Some(n) = head {
            lo += n;
        }
        let tail = phrases
            .iter()
            .find(|p| hi - lo >= p.len() && matches(&words[hi - p.len()..hi], p))
            .map(|p| p.len());
        if let Some(n) = tail {
            hi -= n;
        }
        if head.is_none() && tail.is_none() {
            break;
        }
    }
    if lo == 0 && hi == words.len() {
        return BlacklistVerdict::Keep;
    }
    if lo == hi {
        return BlacklistVerdict::Drop;
    }
    // Keep sentence punctuation that closed the kept text; drop separators left dangling by a removed tail.
    let end = if hi < words.len() { words[hi].1 } else { text.len() };
    let rest = &text[words[lo].1..end];
    let rest = if hi < words.len() {
        rest.trim_end_matches(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | ':' | '-' | '—' | '–' | '…'))
    } else {
        rest.trim_end()
    };
    BlacklistVerdict::Trim(rest.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const BL: &str = "amara.org, subtítulos por, suscríbete, dale like, gracias por ver, \
                      gracias por ver el video, like, thank you for watching";

    fn trim(s: &str) -> BlacklistVerdict {
        BlacklistVerdict::Trim(s.to_string())
    }

    #[test]
    fn middle_and_partial_words_are_kept() {
        assert_eq!(filter_blacklisted("I would like to show you something", BL), BlacklistVerdict::Keep);
        assert_eq!(filter_blacklisted("hoy comemos tacos", "memos, gracias"), BlacklistVerdict::Keep);
        assert_eq!(filter_blacklisted("that is unlikely", BL), BlacklistVerdict::Keep);
        assert_eq!(filter_blacklisted("", BL), BlacklistVerdict::Keep);
    }

    #[test]
    fn whole_utterance_is_dropped() {
        assert_eq!(filter_blacklisted("Gracias por ver.", BL), BlacklistVerdict::Drop);
        assert_eq!(filter_blacklisted("Subtítulos por amara.org", BL), BlacklistVerdict::Drop);
        assert_eq!(filter_blacklisted("¡SUSCRIBETE!", BL), BlacklistVerdict::Drop);
        assert_eq!(filter_blacklisted("Dale like. Suscríbete, gracias por ver.", BL), BlacklistVerdict::Drop);
    }

    #[test]
    fn trailing_phrases_are_stripped() {
        assert_eq!(filter_blacklisted("y eso es todo, gracias por ver el video", BL), trim("y eso es todo"));
        assert_eq!(filter_blacklisted("Hola. Gracias por ver.", BL), trim("Hola."));
        // stacked tails
        assert_eq!(
            filter_blacklisted("eso es todo. Gracias por ver. Suscríbete", BL),
            trim("eso es todo.")
        );
    }

    #[test]
    fn leading_phrases_are_stripped() {
        assert_eq!(filter_blacklisted("Suscríbete, hoy vamos a ver Rust", BL), trim("hoy vamos a ver Rust"));
        assert_eq!(filter_blacklisted("Dale like. Gracias por ver. Hola ¿qué tal?", BL), trim("Hola ¿qué tal?"));
    }

    #[test]
    fn both_edges_and_middle() {
        assert_eq!(
            filter_blacklisted("Suscríbete, y dale like a este video, gracias por ver", BL),
            trim("y dale like a este video")
        );
    }

    #[test]
    fn empty_blacklist_keeps_everything() {
        assert_eq!(filter_blacklisted("amara.org", ""), BlacklistVerdict::Keep);
        assert_eq!(filter_blacklisted("amara.org", "   ,  ; "), BlacklistVerdict::Keep);
    }

    #[test]
    fn quotes_and_separators_in_the_list() {
        let bl = "“amara.org”\n'suscribete';«dale like»";
        assert_eq!(filter_blacklisted("amara.org", bl), BlacklistVerdict::Drop);
        assert_eq!(filter_blacklisted("hola, Suscríbete", bl), trim("hola"));
    }
}
