use std::collections::HashMap;

pub mod huffman;

/// One branch of a decoded internal node.
///
/// `TABLE` stores, for every internal node, the pair `(bit=0, bit=1)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// A leaf: emit this character and jump back to the root.
    Str(char),
    /// The fallback leaf: read one raw UTF-8 sequence from the bit stream.
    Escape,
    /// Follow this edge to another internal node (index into `TABLE`).
    Jmp(u32),
}

pub mod tree {
    use crate::Op;
    include!("generated/tree.rs");
}

#[derive(Debug, Clone, Default)]
pub struct CharStats {
    counts: HashMap<char, u64>,
    total: u64,
}

impl CharStats {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_str(&mut self, text: &str) -> u64 {
        let mut added = 0;
        for ch in text.chars() {
            *self.counts.entry(ch).or_insert(0) += 1;
            added += 1;
        }
        self.total += added;
        added
    }

    pub fn merge(&mut self, other: &CharStats) {
        for (&ch, &n) in &other.counts {
            *self.counts.entry(ch).or_insert(0) += n;
        }
        self.total += other.total;
    }

    pub fn total(&self) -> u64 {
        self.total
    }

    pub fn distinct(&self) -> usize {
        self.counts.len()
    }

    pub fn get(&self, ch: char) -> u64 {
        self.counts.get(&ch).copied().unwrap_or(0)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&char, &u64)> {
        self.counts.iter()
    }

    pub fn sorted(&self) -> Vec<(char, u64)> {
        let mut v: Vec<(char, u64)> = self.counts.iter().map(|(&c, &n)| (c, n)).collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        v
    }
}

pub fn count_chars(text: &str) -> CharStats {
    let mut stats = CharStats::new();
    stats.add_str(text);
    stats
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_chars() {
        let stats = count_chars("aab粘粘");
        assert_eq!(stats.total(), 5);
        assert_eq!(stats.distinct(), 3);
        assert_eq!(stats.get('a'), 2);
        assert_eq!(stats.get('b'), 1);
        assert_eq!(stats.get('粘'), 2);
    }

    #[test]
    fn sorted_is_descending() {
        let stats = count_chars("abbccc");
        let sorted = stats.sorted();
        assert_eq!(sorted[0], ('c', 3));
        assert_eq!(sorted[1], ('b', 2));
        assert_eq!(sorted[2], ('a', 1));
    }

    #[test]
    fn merge_accumulates() {
        let mut a = count_chars("abc");
        a.merge(&count_chars("bc"));
        assert_eq!(a.total(), 5);
        assert_eq!(a.get('b'), 2);
    }
}
