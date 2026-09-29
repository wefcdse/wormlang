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

use std::collections::HashSet;

use crate::huffman::Codebook;

#[derive(Debug, Clone)]
pub struct WordCodec {
    words: Vec<String>,
    block_words: usize,
    block_bits: usize,
}

impl WordCodec {
    /// Build a codec for the given word list.
    ///
    /// Words must be distinct, non-empty and prefix-free (no word may be a
    /// prefix of another), so that a run of words can be split unambiguously.
    pub fn new(words: Vec<String>) -> Result<Self, String> {
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
        for a in &words {
            for b in &words {
                if a != b && b.starts_with(a.as_str()) {
                    return Err(format!("word {a:?} is a prefix of {b:?}"));
                }
            }
        }

        let (block_words, block_bits) = choose_block(words.len());
        Ok(Self {
            words,
            block_words,
            block_bits,
        })
    }

    pub fn words(&self) -> &[String] {
        &self.words
    }

    /// `(block_words, block_bits)`: how many words carry how many bits.
    pub fn block(&self) -> (usize, usize) {
        (self.block_words, self.block_bits)
    }

    /// Words per bit, i.e. how efficiently the block packs bits.
    pub fn bits_per_word(&self) -> f64 {
        self.block_bits as f64 / self.block_words as f64
    }

    /// Encode a bit stream into word text. The stream is zero-padded to a whole
    /// block; a terminated stream (ending in its EOF code) makes the padding
    /// harmless.
    pub fn encode_bits(&self, bits: &[bool]) -> String {
        if bits.is_empty() {
            return String::new();
        }
        let n = self.words.len() as u64;
        let mut out = String::new();
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
                out.push_str(&self.words[digit]);
            }
            if i >= bits.len() {
                break;
            }
        }
        out
    }

    /// Decode word text back into a bit stream.
    pub fn decode_bits(&self, text: &str) -> Result<Vec<bool>, String> {
        let digits = self.tokenize(text)?;
        if digits.len() % self.block_words != 0 {
            return Err(format!(
                "word count {} is not a multiple of the block size {}",
                digits.len(),
                self.block_words
            ));
        }
        let n = self.words.len() as u64;
        let mut bits = Vec::with_capacity(digits.len() / self.block_words * self.block_bits);
        for block in digits.chunks(self.block_words) {
            let mut value = 0u64;
            for &digit in block {
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

    fn tokenize(&self, text: &str) -> Result<Vec<usize>, String> {
        let mut digits = Vec::new();
        let mut rest = text;
        while !rest.is_empty() {
            match self
                .words
                .iter()
                .position(|word| rest.starts_with(word.as_str()))
            {
                Some(index) => {
                    rest = &rest[self.words[index].len()..];
                    digits.push(index);
                }
                None => {
                    let tail: String = rest.chars().take(8).collect();
                    return Err(format!("unknown word near: {tail:?}"));
                }
            }
        }
        Ok(digits)
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
        Ok(Self {
            codebook,
            words: WordCodec::new(words)?,
        })
    }

    pub fn encode(&self, text: &str) -> String {
        self.words.encode_bits(&self.codebook.encode_terminated(text))
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
        let (words, bits) = choose_block(3);
        assert_eq!((words, bits), (7, 11));
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
    fn full_encoding_roundtrips_with_three_words() {
        let cb = codebook("粘粘虫扁扁虫西瓜虫粘粘虫西瓜虫~ hello world");
        let enc = Encoding::new(
            cb,
            vec!["粘粘虫".into(), "扁扁虫".into(), "西瓜虫".into()],
        )
        .unwrap();
        let text = "粘粘虫 西瓜虫 hello 鿿🦑 end";
        let worms = enc.encode(text);
        assert!(worms.chars().all(|c| c == '粘' || c == '扁' || c == '虫' || c == '西' || c == '瓜'));
        assert_eq!(enc.decode(&worms).unwrap(), text);
    }
}
