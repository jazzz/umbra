use tracing::info;

use serde::{Deserialize, Serialize};
use umbra_content_types::{ChatMessage, Message, content_types::types::ContentTags};
use umbra_sdk::{ContentFrame, TaggedContent, UmbraClient};

// User defined Message
#[derive(Debug, Serialize, Deserialize)]
struct UrlMessage {
    url: String,
    text: String,
}

impl TaggedContent for UrlMessage {
    const TAG: u32 = 6;
}

fn print_content(client: &str, conversation_id: String, content: ContentFrame) {
    match content.tag {
        val if val == ContentTags::ContentTagChatMessage as u32 => {
            let msg: ChatMessage = content.bytes.into();
            info!("{} Recv({}): {:?}", client, conversation_id, msg);
        }
        val if val == UrlMessage::TAG => {
            let msg: UrlMessage = bincode::deserialize(&content.bytes).unwrap();
            info!("{} Recv({}): {:?}", client, conversation_id, msg);
        }
        _ => {
            info!("Unknown content type with tag: {}", content.tag);
        }
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO) // Set the maximum log level
        .with_target(false)
        .init();

    info!("Starting Umbra POC");

    let mut amal = UmbraClient::new();
    amal.add_content_handler(|convo, content_frame| print_content("Amal", convo, content_frame));
    let a2b = amal.create_conversation("bola_address".into()).unwrap();

    let msg = ChatMessage {
        text: "Hello Bola!".to_string(),
    }
    .encode_to_vec();

    let hello_bytes = a2b.lock().unwrap().send(5, msg);
    amal.recv(hello_bytes.as_slice())
        .expect("msg decode failed");

    // User Defined using custom encoding
    let url = UrlMessage {
        url: "https://example.com".to_string(),
        text: "Check this out!".to_string(),
    };

    let hello_bytes = a2b
        .lock()
        .unwrap()
        .send(UrlMessage::TAG, bincode::serialize(&url).unwrap());

    amal.recv(hello_bytes.as_slice())
        .expect("msg decode failed");
}
