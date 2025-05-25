use std::fmt::Debug;

use prost::Message;
pub use types::{
    ApplicationFrameV1, ChatMessage, ConfidentialFrame, Contact, EncryptedBytes, Frame,
    PublicFrame, ReliabilityInfo, application_frame_v1, confidential_frame,
    encrypted_bytes::{self, Aes256Ctr},
    frame, public_frame,
};
use types::{PayloadTags, ProtocolTags, TaggedPayload};

pub mod types {
    include!(concat!(env!("OUT_DIR"), "/umbra.types.rs"));
}

pub enum ConvoType {
    Public,
    Session,
    Group,
}

// Marker trait to tag message types which are safe to send out on the wire.
// All send functions require types to implement this trait.
trait SafeToSend: Debug {}

impl SafeToSend for EncryptedBytes {}
impl SafeToSend for PublicFrame {}

pub trait ToFrame {
    fn to_frame(self, reliability_info: Option<ReliabilityInfo>) -> Frame;
}

pub trait ToPayload {
    fn to_payload(self) -> TaggedPayload;
}

impl ToFrame for ChatMessage {
    fn to_frame(self, reliability_info: Option<ReliabilityInfo>) -> Frame {
        Frame {
            reliability_info: reliability_info,
            frame_type: Some(frame::FrameType::ConfidentialFrame(ConfidentialFrame {
                r#type: Some(confidential_frame::Type::AppFrameV1(ApplicationFrameV1 {
                    payload: Some(application_frame_v1::Payload::ChatMsg(self)),
                })),
            })),
        }
    }
}

impl ToFrame for Contact {
    fn to_frame(self, reliability_info: Option<ReliabilityInfo>) -> Frame {
        Frame {
            reliability_info: reliability_info,
            frame_type: Some(frame::FrameType::PublicFrameFrame(PublicFrame {
                frame_type: Some(public_frame::FrameType::Contact(self)),
            })),
        }
    }
}

impl ToPayload for EncryptedBytes {
    fn to_payload(self) -> TaggedPayload {
        TaggedPayload {
            protocol: ProtocolTags::UmbraV1 as u32,
            tag: PayloadTags::TagEncryptedFrame as u32,
            payload_bytes: self.encode_to_vec(),
        }
    }
}

mod tests {
    use super::*;
    use crate::encrypted_bytes::Aes256Ctr;
    use crate::encrypted_bytes::Algo;

    // fn send(packet: impl MustEncrypt) {
    //     // This function is a placeholder for sending the encrypted message
    //     // In a real implementation, this would send the encrypted message over the network

    //     println!("{:?}", packet)
    // }

    // #[test]
    // fn test_encrypted_bytes() {
    //     let a = ChatMessage {
    //         text: "hello".into(),
    //         message_id: 1.to_string(),
    //     };

    //     let a = Contact { name: "Bob".into() };

    //     send(a)
    // }
}
