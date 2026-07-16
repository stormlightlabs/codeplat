mod analysis;
mod model;
mod render;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::time::SystemTime;

use render::Render;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::cli::{ColorPolicy, CommandRequest, OutputFormat};
use crate::utils::token_count;
use crate::{history, map, security, utils};

pub use model::*;

/// The current compatibility version of the JSON report envelope.
pub const SCHEMA_VERSION: u16 = 1;
/// The default trailing period used for churn, bug, and firefighting signals.
pub const DEFAULT_HISTORY_WINDOW_DAYS: u32 = 365;
/// The default trailing period used for recent contributor concentration.
pub const DEFAULT_RECENT_WINDOW_DAYS: u32 = 180;
/// The default case-insensitive bug-related commit-message keywords.
pub const DEFAULT_BUG_KEYWORDS: &[&str] = &["fix", "bug", "broken"];
/// The default case-insensitive firefighting commit-message keywords.
pub const DEFAULT_FIREFIGHTING_KEYWORDS: &[&str] = &["revert", "hotfix", "emergency", "rollback"];
/// The package version embedded in every machine-readable report.
pub const TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");
/// The schema file shipped with Codeplat.
pub const SCHEMA_PATH: &str = "schema/v1/codeplat.json";

fn stable_repository_id(repository_root: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(repository_root.as_bytes());
    format!("sha256:{}", hex_digest(digest.finalize().as_slice()))
}

fn hex_digest(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut output, "{byte:02x}").expect("writing a digest to a string cannot fail");
    }
    output
}
