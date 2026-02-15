//! Lifecycle hooks system.
//!
//! Provides extensible hooks that fire at key points in the agent lifecycle:
//! - `beforeInbound` — Before an incoming message is processed
//! - `beforeOutbound` — Before a response is sent back
//! - `beforeToolCall` — Before a tool is executed
//! - `onSessionStart` — When a new session begins
//! - `onSessionEnd` — When a session expires or is closed
//! - `transformResponse` — Transform the final response text
//! - `onMessage` — When a message is received (already handled by routines)
//! - `transcribeAudio` — Transcribe audio content

pub mod bundled;
mod engine;
pub mod transcribe;
mod types;
pub mod webhooks;

pub use bundled::{all_bundled_hooks, register_bundled_hooks};
pub use engine::HookEngine;
pub use transcribe::{TranscriptionHookResult, is_supported_audio_mime, run_transcribe_audio};
pub use types::{
    Hook, HookAction, HookContext, HookError, HookEvent, HookOutcome, HookPriority,
    HookRegistration, HookSource, HookType, InboundHookResult, OutboundHookResult,
    ToolCallHookResult, TransformResponseResult,
};
