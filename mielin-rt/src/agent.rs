//! Agent execution on embedded targets

pub struct EmbeddedAgent {
    pub id: [u8; 16],
}

impl EmbeddedAgent {
    pub fn new(id: [u8; 16]) -> Self {
        Self { id }
    }
}
