mod crypto;
mod sdk;
mod utils;

use std::{cell::RefCell, collections::HashMap};
use umbra_types::{ChatMessage, Message};

use sdk::{
    Addr, AddrRef, Blob, ClientId, ClientIdRef, ContentTopic, ContentTopicRef, ConversationSession,
    DS, Poll, Publish, Store, UmbraClient, UmbraError,
};

use tracing::{debug, info};
use umbra_types::payload::types::TaggedPayload;

struct SharedStore {
    datastore: HashMap<ContentTopic, Vec<Blob>>, // Stores messages by topic
    client_cursors: HashMap<ClientId, usize>, // Tracks which message was last read by each client
}

// This object uses interior mutability for simplicity in single threaded code
struct LocalDistributionService {
    store: RefCell<SharedStore>,
}

impl LocalDistributionService {
    // Constructor for LocalDistributionService
    fn new() -> Self {
        Self {
            store: RefCell::new(SharedStore {
                datastore: HashMap::new(),
                client_cursors: HashMap::new(),
            }),
        }
    }

    fn get_cursor(&self, client_id: ClientIdRef) -> usize {
        *self
            .store
            .borrow()
            .client_cursors
            .get(client_id)
            .unwrap_or(&0)
    }

    fn update_cursor(&self, client_id: ClientIdRef, cursor: usize) {
        self.store
            .borrow_mut()
            .client_cursors
            .insert(client_id.into(), cursor);
    }
}

impl Publish for LocalDistributionService {
    fn publish(&self, topic: ContentTopic, value: TaggedPayload) {
        debug!("Publish [{}] {:?}", topic, value);
        self.store
            .borrow_mut()
            .datastore
            .entry(topic)
            .or_insert_with(Vec::new)
            .push(value.encode_to_vec());
    }
}

impl Poll for LocalDistributionService {
    // TODO: Remove polling for PubSub
    fn poll(&self, client_id: ClientIdRef, topic: ContentTopicRef) -> Vec<Vec<u8>> {
        let start_index = self.get_cursor(client_id);

        let store = &self.store.borrow_mut().datastore;
        let all_msgs = store.get(topic);
        let x = all_msgs.unwrap();

        //TODO: delete
        let mut y: Vec<Vec<u8>> = x.clone();
        let z: Vec<Vec<u8>> = y.drain(start_index..y.len()).collect();

        self.update_cursor(client_id, x.len());
        return z;
    }
}

impl DS for LocalDistributionService {}

// impl PrintInternals for LocalDistributionService {
//     fn print_internals(&self) {
//         println!("==============Datastore contents=============");
//         let ds = self.datastore.borrow();
//         for (topic, messages) in ds.iter() {
//             println!("Topic: {}", topic);
//             for message in messages {
//                 println!("  Message: {:?}", message);
//             }
//         }
//         println!("==============End Store contents=============");
//     }
// }

struct MsgDb {}
impl Store for MsgDb {}

struct App<'a> {
    client: UmbraClient<'a>,
    convos: HashMap<Addr, Vec<ConversationSession<'a>>>,
}

impl<'a> App<'a> {
    fn address(&self) -> Addr {
        self.client.address().into()
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO) // Set the maximum log level
        .with_target(false)
        .init();

    info!("Starting Umbra POC");

    let ds = LocalDistributionService::new();
    let db1 = MsgDb {};
    let db2 = MsgDb {};

    let mut amal = App {
        client: UmbraClient::create_with_ephemeral_identity(&ds, &db1).unwrap(),
        convos: HashMap::new(),
    };

    let mut bola = App {
        client: UmbraClient::create_with_ephemeral_identity(&ds, &db2).unwrap(),
        convos: HashMap::new(),
    };

    let addr_amal = amal.address();
    let addr_bola = bola.address();

    let a2b = amal.client.create_session(&addr_bola).unwrap();
    let b2a = bola.client.create_session(&addr_amal).unwrap();

    a2b.borrow()
        .send(ChatMessage {
            text: "Hello, Bola!".to_string(),
            message_id: utils::generate_random_string(7),
        })
        .unwrap();

    b2a.borrow()
        .send(ChatMessage {
            text: "Hello, Amal!".to_string(),
            message_id: utils::generate_random_string(7),
        })
        .unwrap();
}
