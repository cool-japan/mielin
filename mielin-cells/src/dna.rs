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
        let mut hash = [0u8; 32];
        if !data.is_empty() {
            hash[0] = data.len() as u8;
        }
        hash
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
}
