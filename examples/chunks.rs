use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use wormlang::huffman::Codebook;

const CHUNK: usize = 20;

fn main() {
    let path = env::args().nth(1).expect("usage: chunks <file-or-dir>");
    let cb = Codebook::from_static(
        wormlang::tree::TABLE,
        wormlang::tree::ENCODE,
        wormlang::tree::ESCAPE,
        wormlang::tree::EOF,
    );

    let mut files = Vec::new();
    collect(Path::new(&path), &mut files);
    files.sort();

    let mut chunks = 0u64;
    let mut total_chars = 0u64;
    let mut total_bits = 0u64;
    let mut min_bits = u64::MAX;
    let mut max_bits = 0u64;
    let mut failures = 0u64;
    let mut samples = Vec::new();

    for file in &files {
        let text = fs::read_to_string(file).unwrap_or_default();
        let chars: Vec<char> = text.chars().collect();
        for piece in chars.chunks(CHUNK) {
            let chunk: String = piece.iter().collect();
            let bits = cb.encode_text(&chunk);
            let decoded = cb.decode_bits(&bits).unwrap_or_default();
            if decoded != chunk {
                failures += 1;
            }
            let n = bits.len() as u64;
            total_chars += chunk.chars().count() as u64;
            total_bits += n;
            chunks += 1;
            min_bits = min_bits.min(n);
            max_bits = max_bits.max(n);
            if samples.len() < 3 && !chunk.trim().is_empty() {
                let preview: String = chunk.chars().take(20).collect();
                samples.push((preview, n));
            }
        }
    }

    println!("files    : {}", files.len());
    println!("chunks   : {chunks}");
    println!("chars    : {total_chars}");
    println!("bits     : {total_bits}");
    println!("bits/char: {:.3}", total_bits as f64 / total_chars as f64);
    println!("chunk bits: min={min_bits} avg={:.1} max={max_bits}", total_bits as f64 / chunks as f64);
    println!("failures : {failures}");
    for (text, bits) in &samples {
        println!("  e.g. {bits:>4} bits  {text}");
    }
}

fn collect(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_dir() {
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                collect(&entry.path(), out);
            }
        }
    } else {
        out.push(path.to_path_buf());
    }
}
