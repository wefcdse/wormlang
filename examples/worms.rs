use std::env;
use std::fs;
use std::io::{self, Read};

use wormlang::huffman::Codebook;
use wormlang::words::Encoding;

fn main() {
    let mut args = env::args().skip(1);
    let mut decode = false;
    let mut first = args.next();
    if first.as_deref() == Some("--decode") {
        decode = true;
        first = args.next();
    }
    let words_csv = first.expect("usage: worms [--decode] <word,word,...> [file]");
    let words: Vec<String> = words_csv.split(',').map(str::to_string).collect();
    let input = args.next();

    let data = match input {
        Some(path) => fs::read_to_string(&path).expect("failed to read input"),
        None => {
            let mut text = String::new();
            io::stdin().read_to_string(&mut text).unwrap();
            text
        }
    };

    let codebook = Codebook::from_static(
        wormlang::tree::TABLE,
        wormlang::tree::ENCODE,
        wormlang::tree::ESCAPE,
        wormlang::tree::EOF,
    );
    let encoding = Encoding::new(codebook, words).expect("bad word list");
    let (block_words, block_bits) = encoding.words.block();
    eprintln!(
        "words={} block={}words/{}bits bits/word={:.4}",
        encoding.words.words().len(),
        block_words,
        block_bits,
        encoding.words.bits_per_word()
    );

    if decode {
        let text = encoding.decode(data.trim()).expect("decode failed");
        print!("{text}");
    } else {
        let worm = encoding.encode(&data);
        eprintln!(
            "input {} chars -> output {} chars",
            data.chars().count(),
            worm.chars().count()
        );
        println!("{worm}");
    }
}
