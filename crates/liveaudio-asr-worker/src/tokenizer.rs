// SPDX-License-Identifier: MIT

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, Deserialize)]
struct AddedToken {
    id: i64,
    content: String,
    #[serde(default)]
    special: bool,
}

#[derive(Debug, Clone)]
pub struct WhisperTokenizer {
    vocab: HashMap<String, i64>,
    id_to_token: HashMap<i64, String>,
    merges: HashMap<(String, String), usize>,
    special_tokens: HashSet<i64>,
    byte_to_char: HashMap<u8, char>,
    char_to_byte: HashMap<char, u8>,
    pub sot_token: i64,
    pub eot_token: i64,
    pub prev_token: i64,
    pub transcribe_token: i64,
    pub notimestamps_token: i64,
    pub nospeech_token: i64,
    pub translate_token: i64,
    pub timestamp_begin: i64,
    pub sot_or_eot_start: i64,
}

impl WhisperTokenizer {
    pub fn from_file(tokenizer_path: &Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(tokenizer_path)
            .map_err(|e| format!("Failed to read tokenizer file {}: {e}", tokenizer_path.display()))?;
        Self::from_json_str(&content)
    }

    pub fn from_json_str(json_str: &str) -> Result<Self, String> {
        let root: Value = serde_json::from_str(json_str)
            .map_err(|e| format!("Failed to parse tokenizer.json: {e}"))?;

        let model = root.get("model").ok_or_else(|| "Missing 'model' object in tokenizer.json".to_string())?;

        let vocab_obj = model
            .get("vocab")
            .and_then(Value::as_object)
            .ok_or_else(|| "Missing 'model.vocab' in tokenizer.json".to_string())?;

        let mut vocab = HashMap::with_capacity(vocab_obj.len() + 1024);
        let mut id_to_token = HashMap::with_capacity(vocab_obj.len() + 1024);

        for (k, v) in vocab_obj {
            if let Some(id) = v.as_i64() {
                vocab.insert(k.clone(), id);
                id_to_token.insert(id, k.clone());
            }
        }

        let mut merges = HashMap::new();
        if let Some(merges_arr) = model.get("merges").and_then(Value::as_array) {
            for (rank, merge_val) in merges_arr.iter().enumerate() {
                if let Some(merge_str) = merge_val.as_str() {
                    let mut parts = merge_str.split_whitespace();
                    if let (Some(a), Some(b)) = (parts.next(), parts.next()) {
                        merges.insert((a.to_string(), b.to_string()), rank);
                    }
                }
            }
        }

        let mut special_tokens = HashSet::new();
        let mut sot_token = 50258;
        let mut eot_token = 50257;
        let mut prev_token = 50361;
        let mut transcribe_token = 50359;
        let mut notimestamps_token = 50363;
        let mut nospeech_token = 50362;
        let mut translate_token = 50358;
        let mut timestamp_begin = 50364;

        if let Some(added_tokens_arr) = root.get("added_tokens").and_then(Value::as_array) {
            for token_val in added_tokens_arr {
                if let Ok(token) = serde_json::from_value::<AddedToken>(token_val.clone()) {
                    vocab.insert(token.content.clone(), token.id);
                    id_to_token.insert(token.id, token.content.clone());
                    if token.special {
                        special_tokens.insert(token.id);
                    }

                    match token.content.as_str() {
                        "<|startoftranscript|>" => sot_token = token.id,
                        "<|endoftext|>" => eot_token = token.id,
                        "<|startofprev|>" => prev_token = token.id,
                        "<|transcribe|>" => transcribe_token = token.id,
                        "<|notimestamps|>" => notimestamps_token = token.id,
                        "<|nocaptions|>" | "<|nospeech|>" => nospeech_token = token.id,
                        "<|translate|>" => translate_token = token.id,
                        "<|0.00|>" => timestamp_begin = token.id,
                        _ => {
                            if token.content.starts_with("<|") && token.content.ends_with("|>") {
                                special_tokens.insert(token.id);
                            }
                        }
                    }
                }
            }
        }

        if let Some(&id) = vocab.get("<|nocaptions|>").or_else(|| vocab.get("<|nospeech|>")) {
            nospeech_token = id;
        }
        if let Some(&id) = vocab.get("<|notimestamps|>") {
            notimestamps_token = id;
        }
        if let Some(&id) = vocab.get("<|startoftranscript|>") {
            sot_token = id;
        }
        if let Some(&id) = vocab.get("<|endoftext|>") {
            eot_token = id;
        }
        if let Some(&id) = vocab.get("<|startofprev|>") {
            prev_token = id;
        }
        if let Some(&id) = vocab.get("<|transcribe|>") {
            transcribe_token = id;
        }
        if let Some(&id) = vocab.get("<|translate|>") {
            translate_token = id;
        }
        if let Some(&id) = vocab.get("<|0.00|>") {
            timestamp_begin = id;
        }

        special_tokens.insert(sot_token);
        special_tokens.insert(eot_token);
        special_tokens.insert(prev_token);
        special_tokens.insert(transcribe_token);
        special_tokens.insert(notimestamps_token);
        special_tokens.insert(nospeech_token);
        special_tokens.insert(translate_token);

        let sot_or_eot_start = eot_token.min(sot_token);

        let (byte_to_char, char_to_byte) = build_byte_to_unicode();

        Ok(Self {
            vocab,
            id_to_token,
            merges,
            special_tokens,
            byte_to_char,
            char_to_byte,
            sot_token,
            eot_token,
            prev_token,
            transcribe_token,
            notimestamps_token,
            nospeech_token,
            translate_token,
            timestamp_begin,
            sot_or_eot_start,
        })
    }

    pub fn language_token_id(&self, lang: &str) -> Option<i64> {
        let token_name = format!("<|{}|>", lang.to_lowercase());
        self.vocab.get(&token_name).copied()
    }

    pub fn is_special_token(&self, id: i64) -> bool {
        id >= self.sot_or_eot_start || self.special_tokens.contains(&id)
    }

    pub fn decode(&self, token_ids: &[i64]) -> String {
        let mut raw_bytes = Vec::with_capacity(token_ids.len() * 4);

        for &id in token_ids {
            if self.is_special_token(id) {
                continue;
            }

            if let Some(token_str) = self.id_to_token.get(&id) {
                for ch in token_str.chars() {
                    if let Some(&b) = self.char_to_byte.get(&ch) {
                        raw_bytes.push(b);
                    } else {
                        let mut buf = [0u8; 4];
                        let encoded = ch.encode_utf8(&mut buf);
                        raw_bytes.extend_from_slice(encoded.as_bytes());
                    }
                }
            }
        }

        String::from_utf8_lossy(&raw_bytes).trim().to_string()
    }

    pub fn encode(&self, text: &str) -> Vec<i64> {
        if text.trim().is_empty() {
            return Vec::new();
        }

        let words = split_text_gpt2(text);
        let mut tokens = Vec::with_capacity(words.len() * 2);

        for word in words {
            let mut byte_chars = String::with_capacity(word.len() * 2);
            for &b in word.as_bytes() {
                if let Some(&ch) = self.byte_to_char.get(&b) {
                    byte_chars.push(ch);
                }
            }

            let bpe_pieces = self.bpe(&byte_chars);
            for piece in bpe_pieces {
                if let Some(&id) = self.vocab.get(&piece) {
                    tokens.push(id);
                }
            }
        }

        tokens
    }

    pub fn encode_prompt(&self, text: &str, max_tokens: usize) -> Vec<i64> {
        let tokens = self.encode(text);
        if tokens.len() > max_tokens {
            let start = tokens.len() - max_tokens;
            tokens[start..].to_vec()
        } else {
            tokens
        }
    }

    fn bpe(&self, token: &str) -> Vec<String> {
        let mut word: Vec<String> = token.chars().map(|c| c.to_string()).collect();
        if word.len() <= 1 {
            return word;
        }

        loop {
            let mut min_rank = usize::MAX;
            let mut best_pair: Option<(String, String)> = None;

            for i in 0..word.len() - 1 {
                let pair = (word[i].clone(), word[i + 1].clone());
                if let Some(&rank) = self.merges.get(&pair) {
                    if rank < min_rank {
                        min_rank = rank;
                        best_pair = Some(pair);
                    }
                }
            }

            let Some((first, second)) = best_pair else {
                break;
            };

            let mut new_word = Vec::with_capacity(word.len());
            let mut i = 0;
            while i < word.len() {
                if i < word.len() - 1 && word[i] == first && word[i + 1] == second {
                    new_word.push(format!("{}{}", first, second));
                    i += 2;
                } else {
                    new_word.push(word[i].clone());
                    i += 1;
                }
            }
            word = new_word;
            if word.len() <= 1 {
                break;
            }
        }

        word
    }
}

pub fn build_byte_to_unicode() -> (HashMap<u8, char>, HashMap<char, u8>) {
    let mut bs = Vec::with_capacity(256);
    for b in b'!'..=b'~' {
        bs.push(b);
    }
    for b in 161..=172 {
        bs.push(b);
    }
    for b in 174..=255 {
        bs.push(b);
    }

    let mut cs: Vec<u32> = bs.iter().map(|&b| b as u32).collect();
    let mut n = 0u32;
    for b in 0..=255u8 {
        if !bs.contains(&b) {
            bs.push(b);
            cs.push(256 + n);
            n += 1;
        }
    }

    let mut byte_to_char = HashMap::with_capacity(256);
    let mut char_to_byte = HashMap::with_capacity(256);

    for (b, c) in bs.into_iter().zip(cs.into_iter()) {
        if let Some(ch) = char::from_u32(c) {
            byte_to_char.insert(b, ch);
            char_to_byte.insert(ch, b);
        }
    }

    (byte_to_char, char_to_byte)
}

fn match_contraction(s: &str) -> Option<usize> {
    if !s.starts_with('\'') {
        return None;
    }
    let after_quote = &s[1..];
    let contractions = ["re", "ve", "ll", "s", "t", "m", "d"];
    for cand in contractions {
        if after_quote.len() >= cand.len()
            && after_quote[..cand.len()].eq_ignore_ascii_case(cand)
        {
            return Some(1 + cand.len());
        }
    }
    None
}

pub fn split_text_gpt2(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut chars = text.char_indices().peekable();

    while let Some(&(start_idx, ch)) = chars.peek() {
        if ch == '\'' {
            let rem = &text[start_idx..];
            if let Some(contraction_len) = match_contraction(rem) {
                words.push(rem[..contraction_len].to_string());
                for _ in 0..contraction_len {
                    chars.next();
                }
                continue;
            }
        }

        if ch == ' ' {
            let mut iter_after = chars.clone();
            iter_after.next();
            if let Some(&(_, next_ch)) = iter_after.peek() {
                if next_ch.is_alphabetic() {
                    let mut chunk = String::new();
                    chunk.push(chars.next().unwrap().1);
                    while let Some(&(_, c)) = chars.peek() {
                        if c.is_alphabetic() {
                            chunk.push(chars.next().unwrap().1);
                        } else {
                            break;
                        }
                    }
                    words.push(chunk);
                    continue;
                } else if next_ch.is_numeric() {
                    let mut chunk = String::new();
                    chunk.push(chars.next().unwrap().1);
                    while let Some(&(_, c)) = chars.peek() {
                        if c.is_numeric() {
                            chunk.push(chars.next().unwrap().1);
                        } else {
                            break;
                        }
                    }
                    words.push(chunk);
                    continue;
                } else if !next_ch.is_whitespace() {
                    let mut chunk = String::new();
                    chunk.push(chars.next().unwrap().1);
                    while let Some(&(idx, c)) = chars.peek() {
                        if c == '\'' && match_contraction(&text[idx..]).is_some() && !chunk.is_empty() {
                            break;
                        }
                        if !c.is_whitespace() && !c.is_alphabetic() && !c.is_numeric() {
                            chunk.push(chars.next().unwrap().1);
                        } else {
                            break;
                        }
                    }
                    words.push(chunk);
                    continue;
                }
            }

            let mut chunk = String::new();
            while let Some(&(_, c)) = chars.peek() {
                if !c.is_whitespace() {
                    break;
                }
                if c == ' ' && !chunk.is_empty() {
                    let mut lookahead = chars.clone();
                    lookahead.next();
                    if let Some(&(_, after_space)) = lookahead.peek() {
                        if !after_space.is_whitespace() {
                            break;
                        }
                    }
                }
                chunk.push(chars.next().unwrap().1);
            }
            words.push(chunk);
        } else if ch.is_whitespace() {
            let mut chunk = String::new();
            while let Some(&(_, c)) = chars.peek() {
                if !c.is_whitespace() {
                    break;
                }
                if c == ' ' && !chunk.is_empty() {
                    let mut lookahead = chars.clone();
                    lookahead.next();
                    if let Some(&(_, after_space)) = lookahead.peek() {
                        if !after_space.is_whitespace() {
                            break;
                        }
                    }
                }
                chunk.push(chars.next().unwrap().1);
            }
            words.push(chunk);
        } else if ch.is_alphabetic() {
            let mut chunk = String::new();
            while let Some(&(_, c)) = chars.peek() {
                if c.is_alphabetic() {
                    chunk.push(chars.next().unwrap().1);
                } else {
                    break;
                }
            }
            words.push(chunk);
        } else if ch.is_numeric() {
            let mut chunk = String::new();
            while let Some(&(_, c)) = chars.peek() {
                if c.is_numeric() {
                    chunk.push(chars.next().unwrap().1);
                } else {
                    break;
                }
            }
            words.push(chunk);
        } else {
            let mut chunk = String::new();
            while let Some(&(idx, c)) = chars.peek() {
                if c == '\'' && match_contraction(&text[idx..]).is_some() && !chunk.is_empty() {
                    break;
                }
                if !c.is_whitespace() && !c.is_alphabetic() && !c.is_numeric() {
                    chunk.push(chars.next().unwrap().1);
                } else {
                    break;
                }
            }
            words.push(chunk);
        }
    }

    words
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_to_unicode_is_bijective_and_covers_all_256_bytes() {
        let (byte_to_char, char_to_byte) = build_byte_to_unicode();
        assert_eq!(byte_to_char.len(), 256);
        assert_eq!(char_to_byte.len(), 256);

        for b in 0..=255u8 {
            let ch = byte_to_char.get(&b).expect("every byte must map");
            let inverted = char_to_byte.get(ch).expect("every char must invert");
            assert_eq!(&b, inverted);
        }
    }

    #[test]
    fn decode_reconstructs_multibyte_utf8_spanish() {
        let (byte_to_char, char_to_byte) = build_byte_to_unicode();
        let spanish_text = "¡Hola, qué tal!";
        let raw_bytes = spanish_text.as_bytes();

        let mut token_str = String::new();
        for &b in raw_bytes {
            token_str.push(*byte_to_char.get(&b).unwrap());
        }

        let mut vocab = HashMap::new();
        let mut id_to_token = HashMap::new();
        vocab.insert(token_str.clone(), 100);
        id_to_token.insert(100, token_str);

        let tokenizer = WhisperTokenizer {
            vocab,
            id_to_token,
            merges: HashMap::new(),
            special_tokens: HashSet::new(),
            byte_to_char,
            char_to_byte,
            sot_token: 50258,
            eot_token: 50257,
            prev_token: 50361,
            transcribe_token: 50359,
            notimestamps_token: 50363,
            nospeech_token: 50362,
            translate_token: 50358,
            timestamp_begin: 50364,
            sot_or_eot_start: 50257,
        };

        let decoded = tokenizer.decode(&[100]);
        assert_eq!(decoded, spanish_text);
    }

    #[test]
    fn decode_filters_special_tokens() {
        let (byte_to_char, char_to_byte) = build_byte_to_unicode();
        let mut vocab = HashMap::new();
        let mut id_to_token = HashMap::new();

        vocab.insert("hello".to_string(), 10);
        id_to_token.insert(10, "hello".to_string());

        let tokenizer = WhisperTokenizer {
            vocab,
            id_to_token,
            merges: HashMap::new(),
            special_tokens: HashSet::new(),
            byte_to_char,
            char_to_byte,
            sot_token: 50258,
            eot_token: 50257,
            prev_token: 50361,
            transcribe_token: 50359,
            notimestamps_token: 50363,
            nospeech_token: 50362,
            translate_token: 50358,
            timestamp_begin: 50364,
            sot_or_eot_start: 50257,
        };

        let decoded = tokenizer.decode(&[50258, 10, 50257]);
        assert_eq!(decoded, "hello");
    }

    #[test]
    fn split_text_gpt2_attaches_space_to_spanish_words() {
        let text = "Transmisión en vivo, charla general, videojuegos y contenido multimedia.";
        let chunks = split_text_gpt2(text);
        assert_eq!(
            chunks,
            vec![
                "Transmisión",
                " en",
                " vivo",
                ",",
                " charla",
                " general",
                ",",
                " videojuegos",
                " y",
                " contenido",
                " multimedia",
                "."
            ]
        );
    }

    #[test]
    fn split_text_gpt2_handles_contractions_and_whitespace() {
        let text = "It's a test   with 123 items.";
        let chunks = split_text_gpt2(text);
        assert_eq!(
            chunks,
            vec![
                "It",
                "'s",
                " a",
                " test",
                "  ",
                " with",
                " 123",
                " items",
                "."
            ]
        );
    }

    #[test]
    fn resolves_nocaptions_and_nospeech_tokens() {
        let json_nocaptions = r#"{
            "model": {
                "vocab": {"hello": 0},
                "merges": []
            },
            "added_tokens": [
                {"id": 50257, "content": "<|endoftext|>", "special": true},
                {"id": 50258, "content": "<|startoftranscript|>", "special": true},
                {"id": 50363, "content": "<|nocaptions|>", "special": true},
                {"id": 50364, "content": "<|notimestamps|>", "special": true}
            ]
        }"#;

        let tok1 = WhisperTokenizer::from_json_str(json_nocaptions).unwrap();
        assert_eq!(tok1.nospeech_token, 50363);

        let json_nospeech = r#"{
            "model": {
                "vocab": {"hello": 0},
                "merges": []
            },
            "added_tokens": [
                {"id": 50257, "content": "<|endoftext|>", "special": true},
                {"id": 50258, "content": "<|startoftranscript|>", "special": true},
                {"id": 50362, "content": "<|nospeech|>", "special": true},
                {"id": 50363, "content": "<|notimestamps|>", "special": true}
            ]
        }"#;

        let tok2 = WhisperTokenizer::from_json_str(json_nospeech).unwrap();
        assert_eq!(tok2.nospeech_token, 50362);
    }

    #[test]
    fn spanish_multiword_round_trip_and_prefix_space() {
        let (byte_to_char, char_to_byte) = build_byte_to_unicode();
        let mut vocab = HashMap::new();
        let mut id_to_token = HashMap::new();
        let mut merges = HashMap::new();
        let mut merge_rank = 0usize;

        let space_char = *byte_to_char.get(&b' ').unwrap();
        assert_eq!(space_char, 'Ġ');

        let to_byte_unicode = |s: &str| -> String {
            s.as_bytes()
                .iter()
                .map(|b| *byte_to_char.get(b).unwrap())
                .collect()
        };

        let mut register_bpe_word = |raw_word: &str, final_id: i64| {
            let encoded_word = to_byte_unicode(raw_word);
            let chars: Vec<String> = encoded_word.chars().map(|c| c.to_string()).collect();
            if chars.len() > 1 {
                let mut acc = chars[0].clone();
                for ch in &chars[1..] {
                    let pair = (acc.clone(), ch.clone());
                    merges.entry(pair).or_insert_with(|| {
                        let r = merge_rank;
                        merge_rank += 1;
                        r
                    });
                    acc.push_str(ch);
                }
            }
            vocab.insert(encoded_word.clone(), final_id);
            id_to_token.insert(final_id, encoded_word);
        };

        register_bpe_word("Transmisión", 1);
        register_bpe_word(" en", 2);
        register_bpe_word(" vivo", 3);
        register_bpe_word(",", 4);
        register_bpe_word(" charla", 5);
        register_bpe_word(" general", 6);
        register_bpe_word(" ", 220);
        register_bpe_word("charla", 7);

        let tokenizer = WhisperTokenizer {
            vocab,
            id_to_token,
            merges,
            special_tokens: HashSet::new(),
            byte_to_char,
            char_to_byte,
            sot_token: 50258,
            eot_token: 50257,
            prev_token: 50361,
            transcribe_token: 50359,
            notimestamps_token: 50363,
            nospeech_token: 50362,
            translate_token: 50358,
            timestamp_begin: 50364,
            sot_or_eot_start: 50257,
        };

        let spanish = "Transmisión en vivo, charla general";
        let encoded = tokenizer.encode(spanish);

        assert_eq!(encoded, vec![1, 2, 3, 4, 5, 6]);
        assert!(!encoded.contains(&220));
        assert!(encoded.contains(&5));
        assert!(tokenizer.id_to_token.get(&5).unwrap().starts_with('Ġ'));

        let decoded = tokenizer.decode(&encoded);
        assert_eq!(decoded, spanish);
    }
}
