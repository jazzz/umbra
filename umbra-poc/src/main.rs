use tracing::info;

use umbra_content_types::{ChatMessage, Message, content_types::types::ContentTags};
use umbra_sdk::{ContentFrame, UmbraClient};

fn print_content(client: &str, conversation_id: String, content: ContentFrame) {
    match content.tag {
        val if val == ContentTags::ContentTagChatMessage as u32 => {
            let msg: ChatMessage = content.bytes.into();
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

    let mut bola = UmbraClient::new();
    bola.add_content_handler(|convo, content_frame| print_content("Bola", convo, content_frame));

    let a2b = amal.create_conversation("bola_address".into()).unwrap();

    let msg = ChatMessage {
        text: "Hello Bola!".to_string(),
    }
    .encode_to_vec();

    let bytes = a2b.lock().unwrap().send(5, msg);

    amal.recv(bytes.as_slice()).expect("msg decode failed");
}
