use bytes::Bytes;
use prost::Message;
use types::{
    ApplicationFrameV1, ChatMessage, ConfidentialFrame, Contact, EncryptedBytes, Frame, PayloadV1,
    PublicFrame, ReliabilityInfo, application_frame_v1, confidential_frame,
    encrypted_bytes::{self, Aes256Ctr},
    frame, payload_v1, public_frame,
};

pub mod types {
    include!(concat!(env!("OUT_DIR"), "/umbra.types.rs"));
}

pub enum ConvoType {
    Public,
    Session,
    Group,
}

pub trait ToFrame {
    fn to_frame(self, reliability_info: Option<ReliabilityInfo>) -> Frame;
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

pub fn encrypt_with_symetric_key(frame: ConfidentialFrame, _key: Bytes) -> EncryptedBytes {
    let bytes = frame.encode_to_vec();
    let mut encrypted_bytes = bytes.clone();
    encrypted_bytes.reverse();

    EncryptedBytes {
        algo: Some(encrypted_bytes::Algo::Aes256ctr(Aes256Ctr {
            encrypted_bytes: encrypted_bytes,
        })),
    }
}

pub fn decrypt(buf: &[u8]) -> Result<Option<Frame>, String> {
    let eb = EncryptedBytes::decode(buf).expect("Failed to parrse Encrypted Bytes");

    let plaintext = match eb
        .algo
        .ok_or_else(|| "No Algo Attached to EncryptedBytes")?
    {
        encrypted_bytes::Algo::Aes256ctr(aes256_ctr) => {
            let mut bytes = aes256_ctr.encrypted_bytes.clone();
            bytes.reverse();
            bytes
        }
        _ => return Err("Cipher not supported".into()),
    };

    let p = plaintext.as_slice();

    let f = Frame::decode(p).map_err(|e| e.to_string())?;

    Ok(Some(f))
}

pub fn send(_convo_type: ConvoType, msg: impl ToFrame) -> Result<Vec<u8>, String> {
    let ee = msg.to_frame(None);

    let payload = if let Some(v) = ee.frame_type {
        match v {
            frame::FrameType::ConfidentialFrame(confidential_frame) => {
                let encrypted_bytes =
                    encrypt_with_symetric_key(confidential_frame, bytes::Bytes::new());

                PayloadV1 {
                    msg_id: "1".into(),
                    frame_type: Some(payload_v1::FrameType::EncryptedFrame(encrypted_bytes)),
                }
            }
            frame::FrameType::PublicFrameFrame(public_frame) => PayloadV1 {
                msg_id: "2".into(),
                frame_type: Some(payload_v1::FrameType::PublicFrame(public_frame)),
            },
        }
    } else {
        return Err(" Invalid encoding - Missing frame contents".into());
    };

    println!("Sending {:?}", payload);

    Ok(payload.encode_to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic() {
        let c = ChatMessage {
            text: "hello".into(),
            message_id: "msg1".into(),
        };

        let _buf = send(ConvoType::Session, c).unwrap();
    }
}
