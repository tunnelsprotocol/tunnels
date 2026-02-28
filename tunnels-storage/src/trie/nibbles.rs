//! Nibble path handling for Merkle Patricia Trie.
//!
//! A nibble is a 4-bit value (0-15). Keys are converted to sequences of nibbles
//! for traversal through the trie. This allows for a 16-way branching factor.

/// A sequence of nibbles (4-bit values).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Nibbles {
    /// The nibble data. Each nibble is stored in the lower 4 bits of a byte.
    data: Vec<u8>,
}

impl Nibbles {
    /// Create an empty nibble sequence.
    pub fn new() -> Self {
        Self { data: Vec::new() }
    }

    /// Create a nibble sequence from raw nibble values.
    pub fn from_raw(nibbles: Vec<u8>) -> Self {
        debug_assert!(nibbles.iter().all(|&n| n < 16), "Nibbles must be < 16");
        Self { data: nibbles }
    }

    /// Convert bytes to nibbles.
    ///
    /// Each byte becomes two nibbles: high nibble first, then low nibble.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut data = Vec::with_capacity(bytes.len() * 2);
        for byte in bytes {
            data.push(byte >> 4);      // High nibble
            data.push(byte & 0x0F);    // Low nibble
        }
        Self { data }
    }

    /// Convert nibbles back to bytes.
    ///
    /// Panics if the number of nibbles is odd.
    pub fn to_bytes(&self) -> Vec<u8> {
        assert!(self.data.len() % 2 == 0, "Cannot convert odd nibbles to bytes");
        self.data
            .chunks(2)
            .map(|pair| (pair[0] << 4) | pair[1])
            .collect()
    }

    /// Get the number of nibbles.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if the sequence is empty.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Get a nibble at a specific index.
    pub fn get(&self, index: usize) -> Option<u8> {
        self.data.get(index).copied()
    }

    /// Get the first nibble.
    pub fn first(&self) -> Option<u8> {
        self.data.first().copied()
    }

    /// Get a slice of nibbles starting at the given index.
    pub fn slice_from(&self, start: usize) -> Self {
        Self {
            data: self.data[start..].to_vec(),
        }
    }

    /// Get a slice of nibbles from start to end (exclusive).
    pub fn slice(&self, start: usize, end: usize) -> Self {
        Self {
            data: self.data[start..end].to_vec(),
        }
    }

    /// Check if this nibble sequence starts with the given prefix.
    pub fn starts_with(&self, prefix: &Nibbles) -> bool {
        if prefix.len() > self.len() {
            return false;
        }
        self.data[..prefix.len()] == prefix.data
    }

    /// Find the common prefix length between two nibble sequences.
    pub fn common_prefix_length(&self, other: &Nibbles) -> usize {
        self.data
            .iter()
            .zip(other.data.iter())
            .take_while(|(a, b)| a == b)
            .count()
    }

    /// Append a single nibble.
    pub fn push(&mut self, nibble: u8) {
        debug_assert!(nibble < 16, "Nibble must be < 16");
        self.data.push(nibble);
    }

    /// Append another nibble sequence.
    pub fn extend(&mut self, other: &Nibbles) {
        self.data.extend_from_slice(&other.data);
    }

    /// Concatenate two nibble sequences.
    pub fn concat(&self, other: &Nibbles) -> Self {
        let mut result = self.clone();
        result.extend(other);
        result
    }

    /// Encode nibbles for storage with a termination flag.
    ///
    /// The encoding packs nibbles into bytes with a prefix that indicates:
    /// - Whether the path is terminated (ends at a leaf value)
    /// - Whether the number of nibbles is odd
    ///
    /// Encoding scheme:
    /// - First nibble encodes flags: bit 0 = terminated, bit 1 = odd length
    /// - If odd length, remaining nibbles are packed directly
    /// - If even length, a padding nibble is added after flags
    pub fn encode(&self, terminated: bool) -> Vec<u8> {
        let odd = self.len() % 2 == 1;
        let flag = (if terminated { 2 } else { 0 }) + (if odd { 1 } else { 0 });

        let mut encoded = Vec::with_capacity((self.len() + 2) / 2);

        if odd {
            // Flag nibble + first nibble in first byte
            encoded.push((flag << 4) | self.data[0]);
            // Pack remaining nibbles
            for chunk in self.data[1..].chunks(2) {
                encoded.push((chunk[0] << 4) | chunk[1]);
            }
        } else {
            // Flag nibble + padding nibble in first byte
            encoded.push(flag << 4);
            // Pack all nibbles
            for chunk in self.data.chunks(2) {
                encoded.push((chunk[0] << 4) | chunk[1]);
            }
        }

        encoded
    }

    /// Decode nibbles from storage format.
    ///
    /// Returns (nibbles, terminated) or None if invalid.
    pub fn decode(encoded: &[u8]) -> Option<(Self, bool)> {
        if encoded.is_empty() {
            return None;
        }

        let flag = encoded[0] >> 4;
        let terminated = (flag & 2) != 0;
        let odd = (flag & 1) != 0;

        let mut data = Vec::new();

        if odd {
            // First byte contains flag nibble + first data nibble
            data.push(encoded[0] & 0x0F);
            // Remaining bytes contain packed nibbles
            for &byte in &encoded[1..] {
                data.push(byte >> 4);
                data.push(byte & 0x0F);
            }
        } else {
            // First byte contains flag nibble + padding nibble (ignored)
            // Remaining bytes contain packed nibbles
            for &byte in &encoded[1..] {
                data.push(byte >> 4);
                data.push(byte & 0x0F);
            }
        }

        Some((Self { data }, terminated))
    }

    /// Get the raw nibble data.
    pub fn as_slice(&self) -> &[u8] {
        &self.data
    }
}

impl Default for Nibbles {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for Nibbles {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for nibble in &self.data {
            write!(f, "{:x}", nibble)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_bytes() {
        let nibbles = Nibbles::from_bytes(&[0xAB, 0xCD]);
        assert_eq!(nibbles.len(), 4);
        assert_eq!(nibbles.get(0), Some(0xA));
        assert_eq!(nibbles.get(1), Some(0xB));
        assert_eq!(nibbles.get(2), Some(0xC));
        assert_eq!(nibbles.get(3), Some(0xD));
    }

    #[test]
    fn test_to_bytes() {
        let nibbles = Nibbles::from_raw(vec![0xA, 0xB, 0xC, 0xD]);
        let bytes = nibbles.to_bytes();
        assert_eq!(bytes, vec![0xAB, 0xCD]);
    }

    #[test]
    fn test_roundtrip() {
        let original = vec![0x12, 0x34, 0x56, 0x78];
        let nibbles = Nibbles::from_bytes(&original);
        let recovered = nibbles.to_bytes();
        assert_eq!(original, recovered);
    }

    #[test]
    fn test_slice_from() {
        let nibbles = Nibbles::from_raw(vec![1, 2, 3, 4, 5]);
        let sliced = nibbles.slice_from(2);
        assert_eq!(sliced, Nibbles::from_raw(vec![3, 4, 5]));
    }

    #[test]
    fn test_starts_with() {
        let nibbles = Nibbles::from_raw(vec![1, 2, 3, 4, 5]);
        assert!(nibbles.starts_with(&Nibbles::from_raw(vec![1, 2])));
        assert!(nibbles.starts_with(&Nibbles::from_raw(vec![1, 2, 3, 4, 5])));
        assert!(!nibbles.starts_with(&Nibbles::from_raw(vec![1, 3])));
        assert!(!nibbles.starts_with(&Nibbles::from_raw(vec![1, 2, 3, 4, 5, 6])));
    }

    #[test]
    fn test_common_prefix_length() {
        let a = Nibbles::from_raw(vec![1, 2, 3, 4, 5]);
        let b = Nibbles::from_raw(vec![1, 2, 3, 6, 7]);
        assert_eq!(a.common_prefix_length(&b), 3);

        let c = Nibbles::from_raw(vec![1, 2, 3, 4, 5]);
        assert_eq!(a.common_prefix_length(&c), 5);

        let d = Nibbles::from_raw(vec![9, 8, 7]);
        assert_eq!(a.common_prefix_length(&d), 0);
    }

    #[test]
    fn test_encode_decode_even() {
        let nibbles = Nibbles::from_raw(vec![1, 2, 3, 4]);

        let encoded = nibbles.encode(false);
        let (decoded, terminated) = Nibbles::decode(&encoded).unwrap();
        assert_eq!(decoded, nibbles);
        assert!(!terminated);

        let encoded = nibbles.encode(true);
        let (decoded, terminated) = Nibbles::decode(&encoded).unwrap();
        assert_eq!(decoded, nibbles);
        assert!(terminated);
    }

    #[test]
    fn test_encode_decode_odd() {
        let nibbles = Nibbles::from_raw(vec![1, 2, 3]);

        let encoded = nibbles.encode(false);
        let (decoded, terminated) = Nibbles::decode(&encoded).unwrap();
        assert_eq!(decoded, nibbles);
        assert!(!terminated);

        let encoded = nibbles.encode(true);
        let (decoded, terminated) = Nibbles::decode(&encoded).unwrap();
        assert_eq!(decoded, nibbles);
        assert!(terminated);
    }

    #[test]
    fn test_encode_decode_empty() {
        let nibbles = Nibbles::new();

        let encoded = nibbles.encode(false);
        let (decoded, terminated) = Nibbles::decode(&encoded).unwrap();
        assert_eq!(decoded, nibbles);
        assert!(!terminated);
    }

    #[test]
    fn test_concat() {
        let a = Nibbles::from_raw(vec![1, 2]);
        let b = Nibbles::from_raw(vec![3, 4]);
        let c = a.concat(&b);
        assert_eq!(c, Nibbles::from_raw(vec![1, 2, 3, 4]));
    }

    #[test]
    fn test_display() {
        let nibbles = Nibbles::from_raw(vec![0xa, 0xb, 0xc, 0xd]);
        assert_eq!(format!("{}", nibbles), "abcd");
    }
}
