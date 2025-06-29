use prost::Message;
use types::*;

pub mod types {
    include!(concat!(env!("OUT_DIR"), "/umbra.contenttypes.rs"));
}

pub trait TaggedContent {
    const TAG: u32;
}

impl ChatMessage {
    pub fn new(text: String) -> Self {
        Self { text }
    }
}

impl TaggedContent for ChatMessage {
    const TAG: u32 = ContentTags::ContentTagChatMessage as u32;
}

impl From<ChatMessage> for Vec<u8> {
    fn from(msg: ChatMessage) -> Self {
        msg.encode_to_vec()
    }
}

impl Into<ChatMessage> for Vec<u8> {
    fn into(self) -> ChatMessage {
        ChatMessage::decode(self.as_slice()).expect("Failed to decode ChatMessage")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_message_new() {
        let chat_message = ChatMessage::new("Hello, World!".to_string());
    }
}
