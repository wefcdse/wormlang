use crate::Op;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, VecDeque};

/// Longest code we are willing to represent in a `u32`.
pub const MAX_CODE_LEN: u8 = 32;

/// Assumed average message length, used to weight the end-of-message leaf: it
/// occurs once per message, so its weight is `total_chars / MESSAGE_LEN`.
pub const MESSAGE_LEN: u64 = 20;

/// A Huffman codebook, laid out for the "sticky worm" bit stream.
#[derive(Debug, Clone)]
pub struct Codebook {
    /// Decode table: `TABLE[state] = (branch_for_bit_0, branch_for_bit_1)`.
    /// The root is index 0; only internal nodes occupy entries.
    pub table: Vec<(Op, Op)>,
    /// Encode table sorted by `char`, for binary search: `(char, code, bit_len)`.
    /// Code is stored MSB-first in the low `bit_len` bits.
    pub encode: Vec<(char, u32, u8)>,
    /// Code of the fallback leaf, `(code, bit_len)`.
    pub escape: Option<(u32, u8)>,
    /// Code of the end-of-message leaf, `(code, bit_len)`.
    pub eof: Option<(u32, u8)>,
}

#[derive(Debug, Clone, Copy)]
enum Node {
    Leaf(char),
    Escape,
    Eof,
    Internal { left: usize, right: usize },
}

/// Build a Huffman codebook from `(char, weight)` pairs.
///
/// An `Escape` leaf is always added so that characters outside the alphabet
/// can still be encoded (as `ESCAPE` followed by their raw UTF-8). Its weight
/// is twice the total count of characters that occur once or twice, on the
/// idea that such rare characters are what a future text is most likely to
/// need the fallback for.
pub fn build_codebook(counts: impl IntoIterator<Item = (char, u64)>) -> Result<Codebook, String> {
    let mut entries: Vec<(char, u64)> = counts.into_iter().collect();
    entries.sort_unstable_by_key(|e| e.0);

    if entries.is_empty() {
        return Err("cannot build a codebook from zero symbols".to_string());
    }

    let escape_weight = entries
        .iter()
        .filter(|(_, weight)| *weight <= 2)
        .map(|(_, weight)| weight)
        .sum::<u64>()
        * 2;

    let total: u64 = entries.iter().map(|(_, weight)| weight).sum();
    let eof_weight = (total / MESSAGE_LEN).max(1);

    let mut nodes: Vec<Node> = Vec::with_capacity(entries.len() * 2 + 1);
    let mut heap: BinaryHeap<Reverse<(u64, usize)>> = BinaryHeap::new();

    for (ch, weight) in entries {
        let idx = nodes.len();
        nodes.push(Node::Leaf(ch));
        heap.push(Reverse((weight, idx)));
    }

    let escape_idx = nodes.len();
    nodes.push(Node::Escape);
    heap.push(Reverse((escape_weight, escape_idx)));

    let eof_idx = nodes.len();
    nodes.push(Node::Eof);
    heap.push(Reverse((eof_weight, eof_idx)));

    while heap.len() > 1 {
        let Reverse((wa, a)) = heap.pop().unwrap();
        let Reverse((wb, b)) = heap.pop().unwrap();
        let idx = nodes.len();
        nodes.push(Node::Internal { left: a, right: b });
        heap.push(Reverse((wa + wb, idx)));
    }

    let root = heap.pop().unwrap().0.1;

    let mut table: Vec<(Op, Op)> = Vec::new();
    let mut index_of: HashMap<usize, usize> = HashMap::new();
    let mut queue: VecDeque<usize> = VecDeque::new();
    index_of.insert(root, 0);
    table.push((Op::Escape, Op::Escape));
    queue.push_back(root);

    while let Some(arena_idx) = queue.pop_front() {
        let table_idx = index_of[&arena_idx];
        let (left, right) = match nodes[arena_idx] {
            Node::Internal { left, right } => (left, right),
            _ => return Err("internal node expected while flattening".to_string()),
        };
        let a = resolve(left, &nodes, &mut index_of, &mut table, &mut queue);
        let b = resolve(right, &nodes, &mut index_of, &mut table, &mut queue);
        table[table_idx] = (a, b);
    }

    let mut encode: Vec<(char, u32, u8)> = Vec::new();
    let mut escape = None;
    let mut eof = None;
    walk(root, 0, 0, &nodes, &mut encode, &mut escape, &mut eof)?;
    encode.sort_unstable_by_key(|e| e.0);

    Ok(Codebook { table, encode, escape, eof })
}

fn resolve(
    arena_idx: usize,
    nodes: &[Node],
    index_of: &mut HashMap<usize, usize>,
    table: &mut Vec<(Op, Op)>,
    queue: &mut VecDeque<usize>,
) -> Op {
    match nodes[arena_idx] {
        Node::Leaf(ch) => Op::Str(ch),
        Node::Escape => Op::Escape,
        Node::Eof => Op::Eof,
        Node::Internal { .. } => {
            if let Some(&table_idx) = index_of.get(&arena_idx) {
                Op::Jmp(table_idx as u32)
            } else {
                let table_idx = table.len();
                index_of.insert(arena_idx, table_idx);
                table.push((Op::Escape, Op::Escape));
                queue.push_back(arena_idx);
                Op::Jmp(table_idx as u32)
            }
        }
    }
}

fn walk(
    idx: usize,
    code: u64,
    len: u8,
    nodes: &[Node],
    encode: &mut Vec<(char, u32, u8)>,
    escape: &mut Option<(u32, u8)>,
    eof: &mut Option<(u32, u8)>,
) -> Result<(), String> {
    match nodes[idx] {
        Node::Leaf(ch) => {
            check_len(len)?;
            encode.push((ch, code as u32, len));
        }
        Node::Escape => {
            check_len(len)?;
            *escape = Some((code as u32, len));
        }
        Node::Eof => {
            check_len(len)?;
            *eof = Some((code as u32, len));
        }
        Node::Internal { left, right } => {
            if len >= MAX_CODE_LEN {
                return Err(format!("code length exceeds {MAX_CODE_LEN} bits"));
            }
            walk(left, code << 1, len + 1, nodes, encode, escape, eof)?;
            walk(right, (code << 1) | 1, len + 1, nodes, encode, escape, eof)?;
        }
    }
    Ok(())
}

fn check_len(len: u8) -> Result<(), String> {
    if len > MAX_CODE_LEN {
        Err(format!("code length {len} exceeds {MAX_CODE_LEN} bits"))
    } else {
        Ok(())
    }
}

impl Codebook {
    /// Wrap tables that were generated ahead of time (see `wormlang-gen`).
    pub fn from_static(
        table: &[(Op, Op)],
        encode: &[(char, u32, u8)],
        escape: (u32, u8),
        eof: (u32, u8),
    ) -> Codebook {
        Codebook {
            table: table.to_vec(),
            encode: encode.to_vec(),
            escape: Some(escape),
            eof: Some(eof),
        }
    }

    /// Encode `text` into a bit stream (MSB-first within each code).
    pub fn encode_text(&self, text: &str) -> Vec<bool> {
        let mut bits = Vec::new();
        let mut buf = [0u8; 4];
        for ch in text.chars() {
            match self.encode.binary_search_by_key(&ch, |e| e.0) {
                Ok(i) => {
                    let (_, code, len) = self.encode[i];
                    push_code(&mut bits, code, len);
                }
                Err(_) => {
                    if let Some((code, len)) = self.escape {
                        push_code(&mut bits, code, len);
                        for &byte in ch.encode_utf8(&mut buf).as_bytes() {
                            push_code(&mut bits, byte as u32, 8);
                        }
                    }
                }
            }
        }
        bits
    }

    /// Encode `text` and append the end-of-message code.
    pub fn encode_terminated(&self, text: &str) -> Vec<bool> {
        let mut bits = self.encode_text(text);
        if let Some((code, len)) = self.eof {
            push_code(&mut bits, code, len);
        }
        bits
    }

    /// Decode a bit stream produced by [`Codebook::encode_text`].
    pub fn decode_bits(&self, bits: &[bool]) -> Result<String, String> {
        self.decode(bits, false)
    }

    /// Decode a bit stream produced by [`Codebook::encode_terminated`],
    /// stopping at the end-of-message code and ignoring any trailing bits.
    pub fn decode_terminated(&self, bits: &[bool]) -> Result<String, String> {
        self.decode(bits, true)
    }

    fn decode(&self, bits: &[bool], stop_at_eof: bool) -> Result<String, String> {
        let mut out = String::new();
        let mut state: u32 = 0;
        let mut i = 0;
        while i < bits.len() {
            let bit = bits[i] as usize;
            i += 1;
            let entry = self
                .table
                .get(state as usize)
                .ok_or_else(|| format!("invalid table state {state}"))?;
            match if bit == 0 { entry.0 } else { entry.1 } {
                Op::Str(ch) => {
                    out.push(ch);
                    state = 0;
                }
                Op::Jmp(target) => state = target,
                Op::Escape => {
                    out.push_str(&read_utf8(bits, &mut i)?);
                    state = 0;
                }
                Op::Eof => {
                    if stop_at_eof {
                        return Ok(out);
                    }
                    return Err("unexpected end-of-message code".to_string());
                }
            }
        }
        if stop_at_eof {
            Err("bit stream ended before the end-of-message code".to_string())
        } else {
            Ok(out)
        }
    }
}

fn push_code(bits: &mut Vec<bool>, code: u32, len: u8) {
    for shift in (0..len).rev() {
        bits.push((code >> shift) & 1 == 1);
    }
}

fn read_bit_byte(bits: &[bool], i: &mut usize) -> Result<u8, String> {
    if *i + 8 > bits.len() {
        return Err("truncated UTF-8 sequence after escape".to_string());
    }
    let mut byte = 0u8;
    for _ in 0..8 {
        byte = (byte << 1) | bits[*i] as u8;
        *i += 1;
    }
    Ok(byte)
}

fn read_utf8(bits: &[bool], i: &mut usize) -> Result<String, String> {
    let first = read_bit_byte(bits, i)?;
    let extra = match first {
        0x00..=0x7F => 0,
        0xC0..=0xDF => 1,
        0xE0..=0xEF => 2,
        0xF0..=0xF7 => 3,
        _ => return Err(format!("invalid UTF-8 lead byte 0x{first:02X}")),
    };
    let mut buf = [0u8; 4];
    buf[0] = first;
    for slot in buf.iter_mut().take(extra + 1).skip(1) {
        *slot = read_bit_byte(bits, i)?;
    }
    std::str::from_utf8(&buf[..=extra])
        .map(str::to_owned)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::count_chars;

    fn codebook(text: &str) -> Codebook {
        build_codebook(count_chars(text).iter().map(|(&c, &n)| (c, n))).unwrap()
    }

    #[test]
    fn roundtrip_within_alphabet() {
        let cb = codebook("hello world, 粘粘虫扁扁虫~ 0123456789");
        let text = "hello 虫虫 world 42";
        assert_eq!(cb.decode_bits(&cb.encode_text(text)).unwrap(), text);
    }

    #[test]
    fn roundtrip_with_escape() {
        let cb = codebook("aaaa");
        let text = "abc粘🦑";
        assert_eq!(cb.decode_bits(&cb.encode_text(text)).unwrap(), text);
    }

    #[test]
    fn every_encoded_char_survives_a_roundtrip() {
        let cb = codebook("the quick brown fox 0123456789");
        for &(ch, _, _) in &cb.encode {
            let bits = cb.encode_text(&ch.to_string());
            assert_eq!(cb.decode_bits(&bits).unwrap(), ch.to_string());
        }
    }

    #[test]
    fn escape_is_a_leaf_of_the_tree() {
        let cb = codebook("abcd");
        assert!(cb.escape.is_some());
        assert_eq!(cb.encode.len(), 4);
    }

    #[test]
    fn generated_tree_roundtrips() {
        let table = crate::tree::TABLE;
        if table.is_empty() {
            return; // placeholder table, nothing to verify yet
        }
        let cb = Codebook::from_static(
            table,
            crate::tree::ENCODE,
            crate::tree::ESCAPE,
            crate::tree::EOF,
        );
        let text = "粘粘虫 和 扁扁虫! Hello 汐~ 鿿🦑 unknown chars";
        assert_eq!(cb.decode_bits(&cb.encode_text(text)).unwrap(), text);
        assert_eq!(
            cb.decode_terminated(&cb.encode_terminated(text)).unwrap(),
            text
        );
    }

    #[test]
    fn terminated_stream_stops_at_eof() {
        let cb = codebook("hello world 粘粘虫");
        let mut bits = cb.encode_terminated("hi 虫");
        bits.extend_from_slice(&[true, false, true, true]);
        assert_eq!(cb.decode_terminated(&bits).unwrap(), "hi 虫");
    }
}
