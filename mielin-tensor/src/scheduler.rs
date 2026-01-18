//! Tensor operation scheduling

pub struct TensorScheduler {}

impl TensorScheduler {
    pub fn new() -> Self {
        Self {}
    }

    pub fn schedule_matmul(&self, _size: usize) {
        // Future: Schedule matrix multiplication
    }
}

impl Default for TensorScheduler {
    fn default() -> Self {
        Self::new()
    }
}
