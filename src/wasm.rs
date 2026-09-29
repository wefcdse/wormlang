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
    words
        .split([',', ';', '\n'])
        .map(|word| word.trim().to_string())
        .filter(|word| !word.is_empty())
        .collect()
}

fn build(words: &str) -> Result<Encoding, JsValue> {
    Encoding::new(codebook(), parse_words(words)).map_err(|e| JsValue::from_str(&e))
}

/// Encode `text` into worm text using the comma-separated `words`.
#[wasm_bindgen]
pub fn encode(text: &str, words: &str) -> Result<String, JsValue> {
    Ok(build(words)?.encode(text))
}

/// Decode worm text back into the original text.
#[wasm_bindgen]
pub fn decode(worms: &str, words: &str) -> Result<String, JsValue> {
    build(words)?.decode(worms).map_err(|e| JsValue::from_str(&e))
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
