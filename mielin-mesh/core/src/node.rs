//! Node representation and management

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type NodeId = Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeRole {
    Edge,
    Relay,
    Core,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    id: NodeId,
    role: NodeRole,
}

impl Node {
    pub fn new(role: NodeRole) -> Self {
        Self {
            id: Uuid::new_v4(),
            role,
        }
    }

    pub fn id(&self) -> &NodeId {
        &self.id
    }

    pub fn role(&self) -> NodeRole {
        self.role
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_creation() {
        let node = Node::new(NodeRole::Edge);
        assert_eq!(node.role(), NodeRole::Edge);
    }
}
