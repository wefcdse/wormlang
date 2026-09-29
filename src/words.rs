//! The "text" layer: turns a bit stream into a string built from a configurable
//! list of words (e.g. `粘粘虫`/`扁扁虫`/`西瓜虫`) and back.
//!
//! This does not map one bit to one word. Instead it treats the bit stream as
//! one big integer and writes it in base `N` (where `N` is the number of
//! words); each base-`N` digit is one word. That way `N = 3` carries
//! `log2(3) ≈ 1.585` bits per word instead of a wasteful 1 bit.
//!
//! Because base `N` is not a power of two in general, the conversion is done in
//! fixed blocks: `block_words` words hold `block_bits = floor(L·log2 N)` bits.
//!
//! On top of that, an optional "frame" wraps the result: a `start` word in
//! front, an `end` word at the back, and a `separator` between everything. For
//! example `start=咩, end=咩, separator=" "` renders `咩 粘粘虫 扁扁虫 … 咩`.

use std::collections::HashSet;

use crate::huffman::Codebook;

#[derive(Debug, Clone)]
pub struct WordCodec {
    words: Vec<String>,
    start: Option<String>,
    end: Option<String>,
    separator: String,
    block_words: usize,
    block_bits: usize,
}

impl WordCodec {
    /// Build a codec for the given word list, with no frame.
    pub fn new(words: Vec<String>) -> Result<Self, String> {
        Self::framed(words, None, None, String::new())
    }

    /// Build a codec with a frame: an optional `start` word, an optional `end`
    /// word, and a `separator` placed between all tokens.
    pub fn framed(
        words: Vec<String>,
        start: Option<String>,
        end: Option<String>,
        separator: String,
    ) -> Result<Self, String> {
        if words.len() < 2 {
            return Err("need at least two words".to_string());
        }
        let mut seen = HashSet::new();
        for word in &words {
            if word.is_empty() {
                return Err("words must be non-empty".to_string());
            }
            if !seen.insert(word.as_str()) {
                return Err(format!("duplicate word: {word}"));
            }
        }

        let start = start.filter(|s| !s.is_empty());
        let end = end.filter(|s| !s.is_empty());
        for frame in [start.as_deref(), end.as_deref()].into_iter().flatten() {
            if seen.contains(frame) {
                return Err(format!("frame word {frame:?} clashes with a data word"));
            }
        }

        if separator.is_empty() {
            // Without a separator every token must be prefix-free.
            let mut all: Vec<&str> = words.iter().map(String::as_str).collect();
            all.extend(start.as_deref());
            all.extend(end.as_deref());
            for (i, a) in all.iter().enumerate() {
                for (j, b) in all.iter().enumerate() {
                    if i != j && b.starts_with(a) {
                        return Err(format!("token {a:?} is a prefix of {b:?}"));
                    }
                }
            }
        } else {
            for token in words
                .iter()
                .map(String::as_str)
                .chain(start.as_deref())
                .chain(end.as_deref())
            {
                if token.contains(&separator) {
                    return Err(format!("token {token:?} contains the separator"));
                }
            }
        }

        let (block_words, block_bits) = choose_block(words.len());
        Ok(Self {
            words,
            start,
            end,
            separator,
            block_words,
            block_bits,
        })
    }

    pub fn words(&self) -> &[String] {
        &self.words
    }

    pub fn start(&self) -> Option<&str> {
        self.start.as_deref()
    }

    pub fn end(&self) -> Option<&str> {
        self.end.as_deref()
    }

    pub fn separator(&self) -> &str {
        &self.separator
    }

    /// `(block_words, block_bits)`: how many words carry how many bits.
    pub fn block(&self) -> (usize, usize) {
        (self.block_words, self.block_bits)
    }

    /// Bits carried per word, i.e. how efficiently the block packs bits.
    pub fn bits_per_word(&self) -> f64 {
        self.block_bits as f64 / self.block_words as f64
    }

    /// Encode a bit stream into word text.
    pub fn encode_bits(&self, bits: &[bool]) -> String {
        let mut tokens: Vec<&str> = Vec::new();
        if let Some(start) = self.start.as_deref() {
            tokens.push(start);
        }
        if !bits.is_empty() {
            let n = self.words.len() as u64;
            let mut i = 0;
            loop {
                let mut value = 0u64;
                for _ in 0..self.block_bits {
                    value <<= 1;
                    if i < bits.len() {
                        if bits[i] {
                            value |= 1;
                        }
                        i += 1;
                    }
                }
                let mut digits = vec![0usize; self.block_words];
                let mut rest = value;
                for slot in digits.iter_mut().rev() {
                    *slot = (rest % n) as usize;
                    rest /= n;
                }
                for digit in digits {
                    tokens.push(&self.words[digit]);
                }
                if i >= bits.len() {
                    break;
                }
            }
        }
        if let Some(end) = self.end.as_deref() {
            tokens.push(end);
        }
        tokens.join(&self.separator)
    }

    /// Decode word text back into a bit stream.
    pub fn decode_bits(&self, text: &str) -> Result<Vec<bool>, String> {
        let mut tokens = self.tokenize(text)?;

        if let Some(start) = self.start.as_deref() {
            if tokens.first().copied() == Some(start) {
                tokens.remove(0);
            } else {
                return Err(format!("expected start word {start:?}"));
            }
        }
        if let Some(end) = self.end.as_deref() {
            if tokens.last().copied() == Some(end) {
                tokens.pop();
            } else {
                return Err(format!("expected end word {end:?}"));
            }
        }

        if tokens.len() % self.block_words != 0 {
            return Err(format!(
                "word count {} is not a multiple of the block size {}",
                tokens.len(),
                self.block_words
            ));
        }

        let n = self.words.len() as u64;
        let mut bits = Vec::with_capacity(tokens.len() / self.block_words * self.block_bits);
        for block in tokens.chunks(self.block_words) {
            let mut value = 0u64;
            for token in block {
                let digit = self
                    .words
                    .iter()
                    .position(|word| word == token)
                    .ok_or_else(|| format!("unknown word: {token:?}"))?;
                value = value * n + digit as u64;
            }
            if value >> self.block_bits != 0 {
                return Err("block value out of range".to_string());
            }
            for shift in (0..self.block_bits).rev() {
                bits.push((value >> shift) & 1 == 1);
            }
        }
        Ok(bits)
    }

    fn tokenize<'a>(&'a self, text: &'a str) -> Result<Vec<&'a str>, String> {
        if !self.separator.is_empty() {
            return Ok(text
                .split(self.separator.as_str())
                .filter(|token| !token.is_empty())
                .collect());
        }

        let mut tokens = Vec::new();
        let mut rest = text;
        while !rest.is_empty() {
            let matched = self
                .start
                .as_deref()
                .filter(|token| rest.starts_with(token))
                .or_else(|| {
                    self.words
                        .iter()
                        .map(String::as_str)
                        .find(|word| rest.starts_with(word))
                })
                .or_else(|| {
                    self.end
                        .as_deref()
                        .filter(|token| rest.starts_with(token))
                });

            match matched {
                Some(token) => {
                    tokens.push(token);
                    rest = &rest[token.len()..];
                }
                None => {
                    let tail: String = rest.chars().take(8).collect();
                    return Err(format!("unknown word near: {tail:?}"));
                }
            }
        }
        Ok(tokens)
    }
}

/// Pick `(L, b)` with `b = floor(L·log2 N)` and `L` words per block. The first
/// block size reaching 99% efficiency wins; otherwise the best one found.
fn choose_block(n: usize) -> (usize, usize) {
    let log2n = (n as f64).log2();
    let mut best = (1usize, 1usize);
    let mut best_ratio = 0.0;
    for block_words in 1..=64usize {
        let exact = block_words as f64 * log2n;
        let block_bits = exact.floor() as usize;
        if block_bits == 0 || block_bits > 60 {
            continue;
        }
        let ratio = block_bits as f64 / exact;
        if ratio >= 0.99 {
            return (block_words, block_bits);
        }
        if ratio > best_ratio {
            best_ratio = ratio;
            best = (block_words, block_bits);
        }
    }
    best
}

/// A full pipeline: characters -> bits (Huffman codebook) -> words.
#[derive(Debug, Clone)]
pub struct Encoding {
    pub codebook: Codebook,
    pub words: WordCodec,
}

impl Encoding {
    pub fn new(codebook: Codebook, words: Vec<String>) -> Result<Self, String> {
        Self::framed(codebook, words, None, None, String::new())
    }

    pub fn framed(
        codebook: Codebook,
        words: Vec<String>,
        start: Option<String>,
        end: Option<String>,
        separator: String,
    ) -> Result<Self, String> {
        Ok(Self {
            codebook,
            words: WordCodec::framed(words, start, end, separator)?,
        })
    }

    pub fn encode(&self, text: &str) -> String {
        self.words
            .encode_bits(&self.codebook.encode_terminated(text))
    }

    pub fn decode(&self, worms: &str) -> Result<String, String> {
        let bits = self.words.decode_bits(worms)?;
        self.codebook.decode_terminated(&bits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::count_chars;

    fn codebook(text: &str) -> Codebook {
        crate::huffman::build_codebook(count_chars(text).iter().map(|(&c, &n)| (c, n))).unwrap()
    }

    #[test]
    fn powers_of_two_are_exact() {
        assert_eq!(choose_block(2), (1, 1));
        assert_eq!(choose_block(4), (1, 2));
        assert_eq!(choose_block(8), (1, 3));
        assert_eq!(choose_block(16), (1, 4));
    }

    #[test]
    fn three_words_use_a_packing_block() {
        assert_eq!(choose_block(3), (7, 11));
    }

    #[test]
    fn bits_roundtrip_through_words() {
        for n in [2usize, 3, 4, 5, 7, 10] {
            let words: Vec<String> = (0..n).map(|i| format!("w{i}")).collect();
            let codec = WordCodec::new(words).unwrap();
            let bits: Vec<bool> = (0..1234u32)
                .map(|i| i.wrapping_mul(2654435761) % 2 == 0)
                .collect();
            let text = codec.encode_bits(&bits);
            let back = codec.decode_bits(&text).unwrap();
            assert!(back.len() >= bits.len());
            assert_eq!(&back[..bits.len()], &bits[..]);
        }
    }

    #[test]
    fn rejects_prefix_words() {
        let err = WordCodec::new(vec!["虫".into(), "虫虫".into()]).unwrap_err();
        assert!(err.contains("prefix"));
    }

    #[test]
    fn frame_roundtrips_with_separator() {
        let codec = WordCodec::framed(
            vec!["粘粘虫".into(), "扁扁虫".into(), "西瓜虫".into()],
            Some("咩".into()),
            Some("咩".into()),
            " ".into(),
        )
        .unwrap();
        let bits: Vec<bool> = (0..500u32).map(|i| i % 3 == 0).collect();
        let text = codec.encode_bits(&bits);
        assert!(text.starts_with("咩 "));
        assert!(text.ends_with(" 咩"));
        assert_eq!(&codec.decode_bits(&text).unwrap()[..bits.len()], &bits[..]);
    }

    #[test]
    fn frame_roundtrips_without_separator() {
        let codec = WordCodec::framed(
            vec!["粘粘虫".into(), "扁扁虫".into()],
            Some("咩".into()),
            Some("啊".into()),
            String::new(),
        )
        .unwrap();
        let bits: Vec<bool> = (0..64u32).map(|i| i % 2 == 0).collect();
        let text = codec.encode_bits(&bits);
        assert!(text.starts_with('咩'));
        assert!(text.ends_with('啊'));
        assert_eq!(&codec.decode_bits(&text).unwrap()[..bits.len()], &bits[..]);
    }

    #[test]
    fn full_encoding_roundtrips_with_frame() {
        let cb = codebook("粘粘虫扁扁虫西瓜虫粘粘虫西瓜虫~ hello world");
        let enc = Encoding::framed(
            cb,
            vec!["粘粘虫".into(), "扁扁虫".into(), "西瓜虫".into()],
            Some("咩".into()),
            Some("咩".into()),
            " ".into(),
        )
        .unwrap();
        let text = "粘粘虫 西瓜虫 hello 鿿🦑 end";
        let worms = enc.encode(text);
        assert_eq!(enc.decode(&worms).unwrap(), text);
    }
}
