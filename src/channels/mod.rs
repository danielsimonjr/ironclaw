//! Multi-channel input system.
//!
//! Channels receive messages from external sources (CLI, HTTP, etc.)
//! and convert them to a unified message format for the agent to process.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────┐
//! │                         ChannelManager                              │
//! │                                                                     │
//! │   ┌──────────────┐   ┌─────────────┐   ┌─────────────┐             │
//! │   │ ReplChannel  │   │ HttpChannel │   │ WasmChannel │   ...       │
//! │   └──────┬───────┘   └──────┬──────┘   └──────┬──────┘             │
//! │          │                 │                 │                      │
//! │          └─────────────────┴─────────────────┘                      │
//! │                            │                                        │
//! │                   select_all (futures)                              │
//! │                            │                                        │
//! │                            ▼                                        │
//! │                     MessageStream                                   │
//! └─────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # WASM Channels
//!
//! WASM channels allow dynamic loading of channel implementations at runtime.
//! See the [`wasm`] module for details.

pub mod block_streamer;
mod channel;
pub mod delivery_retry;
mod http;
pub mod inline_commands;
mod manager;
mod repl;
pub mod self_message;
pub mod status_tracker;
pub mod wasm;
pub mod web;
mod webhook_server;

pub use block_streamer::{BlockStreamConfig, TextBlock};
pub use channel::{Channel, IncomingMessage, MessageStream, OutgoingResponse, StatusUpdate};
pub use delivery_retry::{DeliveryOutcome, DeliveryRetryManager, DeliverySnapshot, RetryConfig};
pub use http::HttpChannel;
pub use inline_commands::{
    CommandCategory, CommandInfo, InlineCommandConfig, ParsedCommand, format_help,
    parse_inline_command,
};
pub use manager::ChannelManager;
pub use repl::ReplChannel;
pub use self_message::SelfMessageFilter;
pub use status_tracker::ChannelStatusTracker;
pub use web::GatewayChannel;
pub use webhook_server::{WebhookServer, WebhookServerConfig};
