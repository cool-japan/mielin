//! Capability-based security sandbox

use std::collections::HashSet;

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum Capability {
    FileSystem,
    Network,
    Camera,
    Gpio,
}

pub struct Sandbox {
    allowed_capabilities: HashSet<Capability>,
}

impl Sandbox {
    pub fn new() -> Self {
        Self {
            allowed_capabilities: HashSet::new(),
        }
    }

    pub fn grant(&mut self, cap: Capability) {
        self.allowed_capabilities.insert(cap);
    }

    pub fn has_capability(&self, cap: &Capability) -> bool {
        self.allowed_capabilities.contains(cap)
    }
}

impl Default for Sandbox {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_granting() {
        let mut sandbox = Sandbox::new();
        assert!(!sandbox.has_capability(&Capability::Camera));

        sandbox.grant(Capability::Camera);
        assert!(sandbox.has_capability(&Capability::Camera));
    }
}
