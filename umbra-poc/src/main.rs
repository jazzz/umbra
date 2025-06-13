use std::{
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use tracing::{debug, error, info};

use serde::{Deserialize, Serialize};
use umbra_content_types::{ChatMessage, Message, content_types::types::ContentTags};
use umbra_sdk::{Blob, ContentFrame, DeliveryService, TaggedContent, UmbraClient};

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

struct QueueSub {
    outbound: Vec<std::sync::mpsc::Sender<Blob>>,
    inbound: Vec<std::sync::Arc<std::sync::Mutex<std::sync::mpsc::Receiver<Blob>>>>,
}

impl QueueSub {
    pub fn new() -> Self {
        QueueSub {
            inbound: Vec::new(),
            outbound: Vec::new(),
        }
    }

    pub fn start(&self) {
        let in_queues = self.inbound.clone();
        let out_queues = self.outbound.clone();
        thread::spawn(move || {
            loop {
                for rx in in_queues.iter() {
                    let rx = rx.lock().unwrap();
                    // This is a placeholder for actual message processing logic

                    match rx.try_recv() {
                        Ok(blob) => {
                            debug!("Processing message: {:?}", blob);
                            for tx in out_queues.iter() {
                                if let Err(e) = tx.send(blob.clone()) {
                                    tracing::error!("Failed to send message: {}", e);
                                }
                            }
                            // Here you would typically process the message
                        }
                        Err(std::sync::mpsc::TryRecvError::Empty) => {
                            // No messages to process, continue to the next queue
                            continue;
                        }
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                            error!("Receiver disconnected, stopping processing.");
                            return; // Exit the loop if the receiver is disconnected
                        }
                    }
                }
            }
        });
    }

    pub fn register(&mut self) -> QueueSubscription {
        let (tx, recv) = std::sync::mpsc::channel();
        let (sender, rx) = std::sync::mpsc::channel();

        self.outbound.push(tx);
        self.inbound
            .push(std::sync::Arc::new(std::sync::Mutex::new(rx)));
        // This is a placeholder for actual subscription logic
        QueueSubscription {
            reciever: Arc::new(Mutex::new(recv)),
            sender: Arc::new(Mutex::new(sender)),
        }
    }
}

struct QueueSubscription {
    reciever: Arc<Mutex<std::sync::mpsc::Receiver<Blob>>>,
    sender: Arc<Mutex<std::sync::mpsc::Sender<Blob>>>,
}

impl DeliveryService for QueueSubscription {
    fn send(&self, message: Blob) -> Result<(), umbra_sdk::UmbraError> {
        debug!("Sending message: {:?}", message);
        self.sender.lock().unwrap().send(message).map_err(|e| {
            umbra_sdk::UmbraError::PublishError(format!("Failed to send message: {}", e))
        })
    }

    fn recv(&self) -> Result<Option<Blob>, umbra_sdk::UmbraError> {
        match self.reciever.lock().unwrap().try_recv() {
            Ok(blob) => {
                debug!("Received message: {:?}", blob);
                Ok(Some(blob))
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => Ok(None),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Err(
                umbra_sdk::UmbraError::PollError("Receiver disconnected".to_string()),
            ),
        }
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO) // Set the maximum log level
        .with_target(false)
        .init();

    info!("Starting Umbra POC");

    // Create a Delivery Service
    let mut queue_sub = QueueSub::new();
    let amal_client = queue_sub.register();
    let bola_client = queue_sub.register();
    let mut amal = UmbraClient::new(amal_client);
    let bola = UmbraClient::new(bola_client);
    amal.add_content_handler(|convo, content_frame| print_content("Amal", convo, content_frame));
    let a2b = amal.create_conversation("bola_address".into()).unwrap();

    amal.start();
    bola.start();
    queue_sub.start();

    let msg = ChatMessage {
        text: "Hello Bola!".to_string(),
    }
    .encode_to_vec();

    a2b.lock().unwrap().send(5, msg);

    // User Defined using custom encoding
    let url = UrlMessage {
        url: "https://example.com".to_string(),
        text: "Check this out!".to_string(),
    };

    a2b.lock()
        .unwrap()
        .send(UrlMessage::TAG, bincode::serialize(&url).unwrap());

    thread::sleep(Duration::from_secs(20));
}
