use prost::Message;
use types::frame::{self};
pub use types::{
    ContentFrame, ConversationInvite, EncryptedBytes, Envelope, Frame, ReliabilityInfo,
};

use types::*;

pub mod types {
    include!(concat!(env!("OUT_DIR"), "/umbra.types.rs"));
}

impl ConversationInvite {
    pub fn new(participants: Vec<String>) -> Self {
        ConversationInvite { participants }
    }
}

///////////////////////////////////////////////////////////////////////////

impl From<u32> for PayloadTags {
    fn from(value: u32) -> Self {
        match value {
            0 => PayloadTags::Uknown,
            1 => PayloadTags::TagEnvelope,
            2 => PayloadTags::TagPublicFrame,
            _ => panic!("Unknown PayloadTag value: {}", value),
        }
    }
}

pub enum ConvoType {
    Public,
    Session,
    Group,
}

pub trait TaggedContent {
    const TAG: u32;
}

pub trait Content {
    fn wrap(self) -> ContentFrame;
}

pub trait ToFrame {
    fn to_frame(self, reliability_info: Option<ReliabilityInfo>) -> Frame;
}

pub trait ToPayload {
    fn to_payload(self) -> TaggedPayload;
}

impl<T: TaggedContent + Into<Vec<u8>>> Content for T {
    fn wrap(self) -> ContentFrame {
        ContentFrame {
            domain: 0,
            tag: <T as TaggedContent>::TAG,
            bytes: self.into(),
        }
    }
}

impl<T: Content> ToFrame for T {
    fn to_frame(self, reliability_info: Option<ReliabilityInfo>) -> Frame {
        Frame {
            reliability_info,
            frame_type: Some(frame::FrameType::Content(self.wrap())),
        }
    }
}

impl ToFrame for ConversationInvite {
    fn to_frame(self, reliability_info: Option<ReliabilityInfo>) -> Frame {
        Frame {
            reliability_info: reliability_info,
            frame_type: Some(frame::FrameType::ConversationInvite(self)),
        }
    }
}

impl ToPayload for Envelope {
    fn to_payload(self) -> TaggedPayload {
        TaggedPayload {
            protocol: ProtocolTags::UmbraV1 as u32,
            tag: PayloadTags::TagEnvelope as u32,
            payload_bytes: self.encode_to_vec(),
        }
    }
}
