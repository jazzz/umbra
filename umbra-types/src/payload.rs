use prost::Message;
use types::frame::{self};
pub use types::{
    ContentFrame, ConversationInvite, EncryptedBytes, Envelope, Frame, ReliabilityInfo,
};

use types::*;

pub mod types {
    include!(concat!(env!("OUT_DIR"), "/umbra.types.rs"));
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

// // Marker trait to tag message types which are safe to send out on the wire.
// // All send functions require types to implement this trait.
// trait SafeToSend: Debug {}

// impl SafeToSend for EncryptedBytes {}

pub trait ToFrame {
    fn to_frame(self, reliability_info: Option<ReliabilityInfo>) -> Frame;
}

pub trait ToPayload {
    fn to_payload(self) -> TaggedPayload;
}

impl ToFrame for ConversationInvite {
    fn to_frame(self, reliability_info: Option<ReliabilityInfo>) -> Frame {
        Frame {
            reliability_info: reliability_info,
            frame_type: Some(frame::FrameType::ConversationInvite(ConversationInvite {})),
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
