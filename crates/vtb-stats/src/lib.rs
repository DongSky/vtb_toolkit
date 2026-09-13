//! vtb-stats — SC/舰长记录簿与场次统计报告.
//!
//! Persists normalized [`vtb_common::LiveEvent`]s into a SQLite database keyed
//! by session id, then aggregates them into a [`SessionReport`] which can be
//! exported as Markdown or CSV.

mod db;
mod error;
mod export;
mod report;
mod tokenize;
mod users;

pub use db::StatsDb;
pub use error::{Result, StatsError};
pub use export::{export_markdown, export_markdown_localized, export_superchats_csv};
pub use report::{GuardRecord, ScRecord, SessionReport};
pub use tokenize::tokenize;
pub use users::{note_key, UserNote, UserProfile};
