pub mod error;
pub mod models;
pub mod rest;
pub mod websocket;

pub use crate::error::SdkError;
pub use crate::models::*;
pub use crate::rest::{OpinionClient, OpinionClientBuilder};
pub use crate::websocket::OpinionWsClient;
