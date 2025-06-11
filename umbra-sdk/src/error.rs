use thiserror::Error;

#[derive(Debug, Error)]
pub enum UmbraError {
    #[error("Failed to publish message to topic: {0}")]
    PublishError(String),

    #[error("Failed to poll messages for client: {0}")]
    PollError(String),

    #[error("Problem encoding type: {0}")]
    EncodingError(String),

    #[error("Problem decoding type: {0}")]
    DecodingError(String),

    #[error("Unknown error occurred")]
    UnexpectedError,

    #[error("Unknown error occurred")]
    TodoError,
}
