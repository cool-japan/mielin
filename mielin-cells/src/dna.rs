//! DNA - WebAssembly binary representation

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dna {
    wasm_binary: Vec<u8>,
    hash: [u8; 32],
}

impl Dna {
    pub fn new(wasm_binary: Vec<u8>) -> Self {
        let hash = Self::compute_hash(&wasm_binary);
        Self { wasm_binary, hash }
    }

    fn compute_hash(data: &[u8]) -> [u8; 32] {
        oxicrypto_hash::Sha256.hash_fixed(data)
    }

    pub fn binary(&self) -> &[u8] {
        &self.wasm_binary
    }

    pub fn hash(&self) -> &[u8; 32] {
        &self.hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dna_creation() {
        let dna = Dna::new(vec![0x00, 0x61, 0x73, 0x6d]);
        assert_eq!(dna.binary().len(), 4);
    }

    #[test]
    fn test_dna_hash_is_real_sha256() {
        let wasm_binary = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        let dna = Dna::new(wasm_binary.clone());

        let expected: [u8; 32] = oxicrypto_hash::Sha256.hash_fixed(&wasm_binary);
        assert_eq!(dna.hash(), &expected);

        // Must not be the old length-derived placeholder (hash[0] == len, rest zero).
        assert_ne!(dna.hash()[1..], [0u8; 31]);
    }

    #[test]
    fn test_dna_hash_is_deterministic_for_identical_input() {
        let wasm_binary = vec![0xde, 0xad, 0xbe, 0xef, 0x01, 0x02, 0x03];
        let dna_a = Dna::new(wasm_binary.clone());
        let dna_b = Dna::new(wasm_binary);
        assert_eq!(dna_a.hash(), dna_b.hash());
    }

    #[test]
    fn test_dna_hash_differs_for_different_input() {
        let dna_a = Dna::new(vec![0x00, 0x61, 0x73, 0x6d]);
        let dna_b = Dna::new(vec![0x00, 0x61, 0x73, 0x6e]);
        assert_ne!(dna_a.hash(), dna_b.hash());
    }
}
