//! Hardware accelerator abstraction

pub struct Accelerator {
    pub name: &'static str,
}

impl Accelerator {
    pub const fn new(name: &'static str) -> Self {
        Self { name }
    }
}
