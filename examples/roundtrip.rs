use std::collections::hash_map::DefaultHasher;
use std::env;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use wormlang::huffman::Codebook;

fn fingerprint(bytes: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

fn bin_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("target/wormlang_roundtrip.bin")
}

fn main() {
    let path = env::args().nth(1).expect("usage: roundtrip <text-file>");
    let text = fs::read_to_string(&path).expect("failed to read input file");
    let cb = Codebook::from_static(wormlang::tree::TABLE, wormlang::tree::ENCODE, wormlang::tree::ESCAPE);

    let bits = cb.encode_text(&text);
    let packed = pack(&bits);

    let mut bin = Vec::with_capacity(8 + packed.len());
    bin.extend_from_slice(&(bits.len() as u64).to_le_bytes());
    bin.extend_from_slice(&packed);
    let bin_path = bin_path();
    fs::write(&bin_path, &bin).expect("failed to write bin");

    let read_back = fs::read(&bin_path).expect("failed to read bin back");
    let bit_len = u64::from_le_bytes(read_back[..8].try_into().unwrap()) as usize;
    let restored_bits = unpack(&read_back[8..], bit_len);
    let decoded = cb.decode_bits(&restored_bits).expect("decode failed");

    let stem = Path::new(&path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("decoded");
    let decoded_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!("{stem}.decoded.txt"));
    fs::write(&decoded_path, &decoded).expect("failed to write decoded text");

    let matched = decoded == text;

    println!("file        : {path}");
    println!("text bytes  : {}", text.len());
    println!("bits        : {}", bits.len());
    println!("bin         : {} ({} bytes, incl. 8-byte header)", bin_path.display(), read_back.len());
    println!("ratio       : {:.3} bits/byte", bits.len() as f64 / text.len() as f64);
    println!("bin head    : {:02X?}", &bin[..8.min(bin.len())]);
    println!("decoded     : {}", decoded_path.display());
    println!("orig  fp    : 0x{:016X}", fingerprint(text.as_bytes()));
    println!("decoded fp  : 0x{:016X}", fingerprint(decoded.as_bytes()));
    println!("round-trip  : {}", if matched { "OK ✓" } else { "MISMATCH ✗" });
    assert!(matched);
}

fn pack(bits: &[bool]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bits.len().div_ceil(8));
    let mut byte = 0u8;
    for (i, &bit) in bits.iter().enumerate() {
        byte = (byte << 1) | bit as u8;
        if i % 8 == 7 {
            out.push(byte);
            byte = 0;
        }
    }
    let rem = bits.len() % 8;
    if rem != 0 {
        out.push(byte << (8 - rem));
    }
    out
}

fn unpack(bytes: &[u8], bit_len: usize) -> Vec<bool> {
    let mut bits = Vec::with_capacity(bit_len);
    'outer: for &byte in bytes {
        for shift in (0..8).rev() {
            if bits.len() == bit_len {
                break 'outer;
            }
            bits.push((byte >> shift) & 1 == 1);
        }
    }
    bits
}
