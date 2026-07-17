//! Bilibili live danmaku client.
//!
//! Implements the WebSocket wire protocol used by the B站 live danmaku
//! servers (as described by the `blivedm` project), the HTTP handshake
//! needed to obtain a danmaku server + token, message parsing into the
//! platform-agnostic [`vtb_common::LiveEvent`] type, and an async client
//! that streams events over a channel.

pub mod error;
pub mod protocol;
pub mod messages;
pub mod api;
pub mod client;
pub mod wbi;
pub mod pb;

pub use client::{DanmakuClient, DanmakuClientConfig};
pub use error::DanmakuError;
