//! Protocol state machine

use crate::{Message, WireError};

pub struct ProtocolHandler {}

impl ProtocolHandler {
    pub fn new() -> Self {
        Self {}
    }

    pub fn handle_message(&mut self, _msg: Message) -> Result<Option<Message>, WireError> {
        Ok(None)
    }
}

impl Default for ProtocolHandler {
    fn default() -> Self {
        Self::new()
    }
}
