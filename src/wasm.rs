use wasm_bindgen::prelude::*;

use crate::huffman::Codebook;
use crate::words::{Encoding, WordCodec};

fn codebook() -> Codebook {
    Codebook::from_static(
        crate::tree::TABLE,
        crate::tree::ENCODE,
        crate::tree::ESCAPE,
        crate::tree::EOF,
    )
}

fn parse_words(words: &str) -> Vec<String> {
    // No trimming: a lone space is a legitimate word.
    words
        .split([',', ';'])
        .filter(|word| !word.is_empty())
        .map(str::to_string)
        .collect()
}

fn opt(value: Option<String>) -> Option<String> {
    value.filter(|s| !s.is_empty())
}

fn build(
    words: &str,
    start: Option<String>,
    end: Option<String>,
) -> Result<Encoding, JsValue> {
    Encoding::framed(codebook(), parse_words(words), opt(start), opt(end))
        .map_err(|e| JsValue::from_str(&e))
}

/// Encode `text` into worm text.
///
/// `words` is comma/semicolon/newline separated. `start`/`end` are optional
/// frame words placed at the very front/back; words are concatenated with no
/// separator.
#[wasm_bindgen]
pub fn encode(
    text: &str,
    words: &str,
    start: Option<String>,
    end: Option<String>,
) -> Result<String, JsValue> {
    Ok(build(words, start, end)?.encode(text))
}

/// Decode worm text back into the original text.
#[wasm_bindgen]
pub fn decode(
    worms: &str,
    words: &str,
    start: Option<String>,
    end: Option<String>,
) -> Result<String, JsValue> {
    build(words, start, end)?
        .decode(worms)
        .map_err(|e| JsValue::from_str(&e))
}

/// Human-readable block info, e.g. "7 words/block, 11 bits/block (1.5714 bits/word)".
#[wasm_bindgen]
pub fn block_info(words: &str) -> Result<String, JsValue> {
    let word_codec = WordCodec::new(parse_words(words)).map_err(|e| JsValue::from_str(&e))?;
    let (block_words, block_bits) = word_codec.block();
    Ok(format!(
        "{block_words} words/block, {block_bits} bits/block ({:.4} bits/word)",
        word_codec.bits_per_word()
    ))
}
