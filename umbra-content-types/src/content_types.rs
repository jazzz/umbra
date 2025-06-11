use prost::Message;
use types::*;
use umbra_types::{
    Frame, ReliabilityInfo, ToFrame,
    payload::{ContentFrame, types::frame},
};

pub mod types {
    include!(concat!(env!("OUT_DIR"), "/umbra.contenttypes.rs"));
}

impl ChatMessage {
    pub fn new(text: String) -> Self {
        Self { text }
    }
}

impl ToFrame for ChatMessage {
    fn to_frame(self, reliability_info: Option<ReliabilityInfo>) -> Frame {
        Frame {
            reliability_info: reliability_info,
            frame_type: Some(frame::FrameType::Content(ContentFrame {
                domain: 0,
                tag: 0,
                bytes: self.encode_to_vec(),
            })),
        }
    }
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
