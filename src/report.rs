use std::collections::BTreeMap;
use std::fmt::Write;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::cli::{ColorPolicy, CommandRequest, OutputFormat};
use crate::utils::token_count;
use crate::{history, map, security, utils};

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

/// Controls how much evidence is emitted after analysis completes.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisProfile {
    /// Emit a bounded navigation briefing suitable for the default command.
    #[default]
    Compact,
    /// Emit the largest bounded evidence collections allowed by the resource limits.
    Evidence,
}

/// Why a collection was not emitted in full.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TruncationReason {
    CollectionLimit,
    OutputBudget,
    ResourceLimit,
    Unsupported,
}

/// Counts for an emitted collection. `total` is the observed count before the
/// profile or a resource limit selected the returned sample.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CollectionSummary {
    pub total: usize,
    pub returned: usize,
    pub truncated: bool,
    pub reason: Option<TruncationReason>,
}

impl Default for CollectionSummary {
    fn default() -> Self {
        Self::complete(0)
    }
}

impl CollectionSummary {
    pub fn complete(total: usize) -> Self {
        Self { total, returned: total, truncated: false, reason: None }
    }

    pub fn bounded(total: usize, returned: usize, reason: TruncationReason) -> Self {
        Self {
            total,
            returned: returned.min(total),
            truncated: returned < total,
            reason: (returned < total).then_some(reason),
        }
    }
}

/// Resource ceilings are part of the report so a partial result is
/// interpretable without relying on process-local defaults.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct ReportLimits {
    pub max_files: usize,
    pub max_file_bytes: usize,
    pub max_total_bytes: usize,
    pub max_syntax_depth: usize,
    pub max_symbols_per_file: usize,
    pub max_symbols: usize,
    pub max_candidates_per_reference: usize,
    pub max_edges: usize,
    pub max_findings: usize,
    pub max_commits: usize,
    pub max_history_evidence: usize,
    pub max_elapsed_ms: u64,
    pub max_output_bytes: usize,
}

impl ReportLimits {
    pub const fn for_profile(profile: AnalysisProfile) -> Self {
        let (max_symbols_per_file, max_symbols, max_edges, max_findings, max_history_evidence) = match profile {
            AnalysisProfile::Compact => (128, 20_000, 2_000, 2_000, 128),
            AnalysisProfile::Evidence => (2_048, 100_000, 20_000, 20_000, 2_000),
        };
        Self {
            max_files: 4_096,
            max_file_bytes: 1_024 * 1_024,
            max_total_bytes: 64 * 1_024 * 1_024,
            max_syntax_depth: 2_048,
            max_symbols_per_file,
            max_symbols,
            max_candidates_per_reference: 32,
            max_edges,
            max_findings,
            max_commits: 100_000,
            max_history_evidence,
            max_elapsed_ms: match profile {
                AnalysisProfile::Compact => 30_000,
                AnalysisProfile::Evidence => 120_000,
            },
            max_output_bytes: 8 * 1_024 * 1_024,
        }
    }
}

impl Default for ReportLimits {
    fn default() -> Self {
        Self::for_profile(AnalysisProfile::Compact)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ReportError {
    #[error("{0}")]
    History(#[source] history::HistoryError),
    #[error("{0}")]
    Map(#[source] map::MapError),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryOperation {
    Churn,
    Contributors,
    Bugs,
    Activity,
    Firefighting,
}

impl HistoryOperation {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Churn => "churn",
            Self::Contributors => "contributors",
            Self::Bugs => "bugs",
            Self::Activity => "activity",
            Self::Firefighting => "firefighting",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandName {
    Briefing,
    Map,
    History,
}

impl CommandName {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Briefing => "briefing",
            Self::Map => "map",
            Self::History => "history",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportStatus {
    Foundation,
    Analyzed,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorktreeState {
    Tracked,
    Modified,
    Untracked,
}

impl WorktreeState {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Tracked => "tracked",
            Self::Modified => "modified",
            Self::Untracked => "untracked",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileAnalysisStatus {
    Complete,
    Partial,
}

/// Cache refresh policy used by source-map analysis.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheMode {
    Auto,
    Always,
    Files,
    Manual,
    Disabled,
}

impl CacheMode {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Always => "always",
            Self::Files => "files",
            Self::Manual => "manual",
            Self::Disabled => "no_cache",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheStatus {
    Disabled,
    Hit,
    Miss,
    Refreshed,
    Stale,
}

impl CacheStatus {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Hit => "hit",
            Self::Miss => "miss",
            Self::Refreshed => "refreshed",
            Self::Stale => "stale",
        }
    }
}

impl FileAnalysisStatus {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Partial => "partial",
        }
    }
}

/// The grammar variant used for one source file. JSX and TSX are kept distinct
/// from their base languages so callers never have to infer syntax from a path.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceLanguage {
    Rust,
    #[serde(rename = "javascript")]
    JavaScript,
    #[serde(rename = "javascript_jsx")]
    JavaScriptJsx,
    #[serde(rename = "typescript")]
    TypeScript,
    #[serde(rename = "typescript_tsx")]
    TypeScriptTsx,
    Python,
    Ruby,
    Java,
    #[serde(rename = "c_sharp")]
    CSharp,
}

impl SourceLanguage {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::JavaScript => "javascript",
            Self::JavaScriptJsx => "javascript_jsx",
            Self::TypeScript => "typescript",
            Self::TypeScriptTsx => "typescript_tsx",
            Self::Python => "python",
            Self::Ruby => "ruby",
            Self::Java => "java",
            Self::CSharp => "c_sharp",
        }
    }

    pub const fn display_label(self) -> &'static str {
        match self {
            Self::Rust => "Rust",
            Self::JavaScript => "JavaScript",
            Self::JavaScriptJsx => "JavaScript (JSX)",
            Self::TypeScript => "TypeScript",
            Self::TypeScriptTsx => "TypeScript (TSX)",
            Self::Python => "Python",
            Self::Ruby => "Ruby",
            Self::Java => "Java",
            Self::CSharp => "C#",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolRole {
    Definition,
    Reference,
}

impl SymbolRole {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Definition => "definition",
            Self::Reference => "reference",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Function,
    Struct,
    Enum,
    Trait,
    Type,
    Const,
    Static,
    Module,
    Macro,
    Field,
    Class,
    Method,
    Variable,
    Interface,
    Import,
    Export,
    Identifier,
    Other,
}

impl SymbolKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Struct => "struct",
            Self::Enum => "enum",
            Self::Trait => "trait",
            Self::Type => "type",
            Self::Const => "const",
            Self::Static => "static",
            Self::Module => "module",
            Self::Macro => "macro",
            Self::Field => "field",
            Self::Class => "class",
            Self::Method => "method",
            Self::Variable => "variable",
            Self::Interface => "interface",
            Self::Import => "import",
            Self::Export => "export",
            Self::Identifier => "identifier",
            Self::Other => "other",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OmissionReason {
    IgnoredUntracked,
    UnsupportedLanguage,
    ExplicitExclusion,
    CacheUnavailable,
    Symlink,
    UnsafePath,
    ReadError,
    TraversalError,
    Oversized,
    Binary,
}

impl OmissionReason {
    pub const fn label(self) -> &'static str {
        match self {
            Self::IgnoredUntracked => "ignored_untracked",
            Self::UnsupportedLanguage => "unsupported_language",
            Self::ExplicitExclusion => "explicit_exclusion",
            Self::CacheUnavailable => "cache_unavailable",
            Self::Symlink => "symlink",
            Self::UnsafePath => "unsafe_path",
            Self::ReadError => "read_error",
            Self::TraversalError => "traversal_error",
            Self::Oversized => "oversized",
            Self::Binary => "binary",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MapFindingKind {
    ParseError,
    ParserError,
    AmbiguousReference,
    QueryError,
}

impl MapFindingKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::ParseError => "parse_error",
            Self::ParserError => "parser_error",
            Self::AmbiguousReference => "ambiguous_reference",
            Self::QueryError => "query_error",
        }
    }
}

/// Typed history-analysis inputs that are also reported for reproducibility.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HistorySettings {
    pub window_days: u32,
    pub recent_window_days: u32,
    pub bug_keywords: Vec<String>,
    pub firefighting_keywords: Vec<String>,
}

/// The complete set of inputs which can change an analysis result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EffectiveOptions {
    pub path: String,
    pub format: OutputFormat,
    #[serde(skip)]
    pub color: ColorPolicy,
    pub profile: AnalysisProfile,
    pub strict: bool,
    pub map: EffectiveMapOptions,
    pub history: HistorySettings,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EffectiveMapOptions {
    pub excludes: Vec<String>,
    pub focuses: Vec<String>,
    pub focus_paths: Vec<String>,
    pub map_tokens: usize,
    pub cache_mode: CacheMode,
    pub cache_files: Vec<String>,
}

impl Default for EffectiveMapOptions {
    fn default() -> Self {
        Self {
            excludes: Vec::new(),
            focuses: Vec::new(),
            focus_paths: Vec::new(),
            map_tokens: 1_000,
            cache_mode: CacheMode::Auto,
            cache_files: Vec::new(),
        }
    }
}

impl Default for EffectiveOptions {
    fn default() -> Self {
        Self {
            path: ".".to_owned(),
            format: OutputFormat::Markdown,
            color: ColorPolicy::Never,
            profile: AnalysisProfile::Compact,
            strict: false,
            map: EffectiveMapOptions::default(),
            history: HistorySettings::default(),
        }
    }
}

/// A repository path is kept in Git's slash-separated form. The path policy
/// records that byte-invalid names are rejected before they can be lossy-
/// decoded or merged with another name.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PathRepresentation {
    Utf8SlashSeparated,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PathEncodingPolicy {
    pub representation: PathRepresentation,
    pub invalid_utf8: String,
    pub case_collisions: String,
    pub markdown: String,
}

impl Default for PathEncodingPolicy {
    fn default() -> Self {
        Self {
            representation: PathRepresentation::Utf8SlashSeparated,
            invalid_utf8: "typed_safety_diagnostic".to_owned(),
            case_collisions: "preserved_or_typed_safety_diagnostic".to_owned(),
            markdown: "backticks_and_control_characters_are_escaped".to_owned(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RepositoryIdentity {
    pub canonical_root: String,
    pub stable_id: String,
    pub object_format: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct HeadSnapshot {
    pub reference: Option<String>,
    pub oid: Option<String>,
    pub detached: bool,
    pub unborn: bool,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorktreeSnapshotState {
    Clean,
    Modified,
    Untracked,
    Mixed,
    #[default]
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorktreeSnapshot {
    pub state: WorktreeSnapshotState,
    pub observed: bool,
    pub tracked_files: usize,
    pub modified_files: usize,
    pub untracked_files: usize,
    pub detail: Option<String>,
}

impl Default for WorktreeSnapshot {
    fn default() -> Self {
        Self {
            state: WorktreeSnapshotState::Unknown,
            observed: false,
            tracked_files: 0,
            modified_files: 0,
            untracked_files: 0,
            detail: Some("this command did not inventory the worktree".to_owned()),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CacheProvenance {
    pub mode: CacheMode,
    pub status: CacheStatus,
    pub available: bool,
    pub hits: usize,
    pub misses: usize,
    pub refreshed: usize,
    pub stale: usize,
}

impl Default for CacheProvenance {
    fn default() -> Self {
        Self {
            mode: CacheMode::Disabled,
            status: CacheStatus::Disabled,
            available: false,
            hits: 0,
            misses: 0,
            refreshed: 0,
            stale: 0,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct LanguageProvenance {
    pub grammar: String,
    pub grammar_version: String,
    pub query_pack: String,
    pub query_pack_version: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReportProvenance {
    pub tool_version: String,
    pub captured_at: String,
    pub reference_time: String,
    pub effective_options: EffectiveOptions,
    pub repository: RepositoryIdentity,
    pub head: HeadSnapshot,
    pub worktree: WorktreeSnapshot,
    pub languages: BTreeMap<String, LanguageProvenance>,
    pub cache: CacheProvenance,
    pub path_encoding: PathEncodingPolicy,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history: Option<HistoryProvenance>,
}

impl Default for ReportProvenance {
    fn default() -> Self {
        Self {
            tool_version: TOOL_VERSION.to_owned(),
            captured_at: String::new(),
            reference_time: String::new(),
            effective_options: EffectiveOptions::default(),
            repository: RepositoryIdentity {
                canonical_root: String::new(),
                stable_id: String::new(),
                object_format: "sha1".to_owned(),
            },
            head: HeadSnapshot::default(),
            worktree: WorktreeSnapshot::default(),
            languages: BTreeMap::new(),
            cache: CacheProvenance::default(),
            path_encoding: PathEncodingPolicy::default(),
            history: None,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ObservedDateRange {
    pub start: Option<String>,
    pub end: Option<String>,
    pub basis: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct HistoryTimeBasis {
    pub window_filters: String,
    pub contributor_recent_and_activity: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct CurrentHeadSemantics {
    pub meaning: String,
    pub reference: Option<String>,
    pub oid: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryCompletenessStatus {
    #[default]
    Complete,
    Shallow,
    MissingObjects,
    Partial,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HistoryCompleteness {
    pub status: HistoryCompletenessStatus,
    pub authoritative: bool,
    pub shallow: bool,
    pub missing_objects: usize,
    pub notes: Vec<String>,
}

impl Default for HistoryCompleteness {
    fn default() -> Self {
        Self {
            status: HistoryCompletenessStatus::Complete,
            authoritative: true,
            shallow: false,
            missing_objects: 0,
            notes: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct HistoryProvenance {
    pub observed_date_range: ObservedDateRange,
    pub time_basis: HistoryTimeBasis,
    pub current_head: CurrentHeadSemantics,
    pub completeness: HistoryCompleteness,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StrictIssue {
    Stale,
    Truncated,
    Incomplete,
    Unsupported,
    #[default]
    Partial,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReportQuality {
    pub stale: bool,
    pub truncated: bool,
    pub incomplete: bool,
    pub unsupported: bool,
    pub partial: bool,
    pub strict_issues: Vec<StrictIssue>,
}

impl Default for HistorySettings {
    fn default() -> Self {
        Self {
            window_days: DEFAULT_HISTORY_WINDOW_DAYS,
            recent_window_days: DEFAULT_RECENT_WINDOW_DAYS,
            bug_keywords: DEFAULT_BUG_KEYWORDS
                .iter()
                .map(|keyword| (*keyword).to_owned())
                .collect(),
            firefighting_keywords: DEFAULT_FIREFIGHTING_KEYWORDS
                .iter()
                .map(|keyword| (*keyword).to_owned())
                .collect(),
        }
    }
}

/// The common, versioned report model used by every command and renderer.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Report {
    pub schema_version: u16,
    #[serde(default)]
    pub profile: AnalysisProfile,
    #[serde(default)]
    pub limits: ReportLimits,
    pub command: CommandDescriptor,
    pub scope: ReportScope,
    pub status: ReportStatus,
    pub summary: String,
    #[serde(default)]
    pub provenance: ReportProvenance,
    #[serde(default)]
    pub quality: ReportQuality,
    pub findings: Vec<Finding>,
    pub limitations: Vec<Limitation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history: Option<HistoryReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map: Option<MapReport>,
}

impl Report {
    pub fn analyze(req: CommandRequest) -> Result<Self, ReportError> {
        let captured_at = utils::capture_date(SystemTime::now());
        match req.command.name {
            CommandName::Briefing => {
                let path = req.command.path.clone();
                let history_report =
                    history::analyze(&path, req.history.clone(), None, req.profile).map_err(ReportError::History)?;
                let mut map_settings = req.map.clone();
                map_settings.profile = req.profile;
                let map_report = map::analyze(&path, &map_settings).map_err(ReportError::Map)?;
                let summary = briefing_summary(&history_report, &map_report);
                Ok(Self::from_parts(
                    req,
                    captured_at,
                    summary,
                    Some(history_report),
                    Some(map_report),
                ))
            }
            CommandName::History => {
                let history_report = history::analyze(
                    &req.command.path,
                    req.history.clone(),
                    req.command.operation,
                    req.profile,
                )
                .map_err(ReportError::History)?;
                let summary = format!(
                    "Analyzed {} reachable commits, including {} non-merge commits, within the selected history scope.",
                    history_report.commits_seen, history_report.non_merge_commits_seen
                );
                Ok(Self::from_parts(req, captured_at, summary, Some(history_report), None))
            }
            CommandName::Map => {
                let mut map_settings = req.map.clone();
                map_settings.profile = req.profile;
                let map_report = map::analyze(&req.command.path, &map_settings).map_err(ReportError::Map)?;
                let summary = &map_report.summary();
                Ok(Self::from_parts(
                    req,
                    captured_at,
                    summary.into(),
                    None,
                    Some(map_report),
                ))
            }
        }
    }

    fn from_parts(
        req: CommandRequest, captured_at: String, summary: String, history: Option<HistoryReport>,
        map: Option<MapReport>,
    ) -> Self {
        let scope_path = map
            .as_ref()
            .map(|report| report.scope_path.clone())
            .or_else(|| history.as_ref().map(|report| report.scope_path.clone()))
            .unwrap_or_else(|| ".".to_owned());
        let repository_root = map
            .as_ref()
            .map(|report| report.repository_root.clone())
            .or_else(|| history.as_ref().map(|report| report.repository_root.clone()))
            .unwrap_or_default();
        let head = map
            .as_ref()
            .map(|report| report.head.clone())
            .or_else(|| history.as_ref().map(|report| report.head.clone()))
            .unwrap_or_default();
        let worktree = map.as_ref().map(|report| report.worktree.clone()).unwrap_or_default();
        let cache = map
            .as_ref()
            .map(|report| CacheProvenance {
                mode: report.cache.mode,
                status: report.cache.status,
                available: report.cache.mode != CacheMode::Disabled,
                hits: report.cache.hits,
                misses: report.cache.misses,
                refreshed: report.cache.refreshed.len(),
                stale: report.cache.stale.len(),
            })
            .unwrap_or_default();
        let effective_options = EffectiveOptions {
            path: req.command.path.to_string_lossy().into_owned(),
            format: req.output_format,
            color: req.color_policy,
            profile: req.profile,
            strict: req.strict,
            map: EffectiveMapOptions {
                excludes: req.map.excludes.clone(),
                focuses: req.map.focuses.clone(),
                focus_paths: req.map.focus_paths.clone(),
                map_tokens: req.map.map_tokens,
                cache_mode: req.map.cache_mode,
                cache_files: req.map.cache_files.clone(),
            },
            history: req.history.clone(),
        };
        let quality = report_quality(history.as_ref(), map.as_ref());
        let provenance = ReportProvenance {
            tool_version: TOOL_VERSION.to_owned(),
            captured_at: captured_at.clone(),
            reference_time: captured_at,
            effective_options,
            repository: RepositoryIdentity {
                canonical_root: repository_root.clone(),
                stable_id: stable_repository_id(&repository_root),
                object_format: object_format(&head).to_owned(),
            },
            head,
            worktree,
            languages: language_provenance(map.as_ref()),
            cache,
            path_encoding: PathEncodingPolicy::default(),
            history: history.as_ref().map(|report| report.provenance.clone()),
        };
        Self {
            schema_version: SCHEMA_VERSION,
            profile: req.profile,
            limits: ReportLimits::for_profile(req.profile),
            command: req.command,
            scope: ReportScope { selected_path: scope_path },
            status: ReportStatus::Analyzed,
            summary,
            provenance,
            quality,
            findings: Vec::new(),
            limitations: Vec::new(),
            history,
            map,
        }
    }

    /// Render a report from the shared typed model without parsing or transforming Markdown.
    pub fn render(&self, format: OutputFormat) -> Result<String, serde_json::Error> {
        let output = match format {
            OutputFormat::Markdown => Ok(self.render_markdown()),
            OutputFormat::Json => {
                let mut output = serde_json::to_string_pretty(self)?;
                output.push('\n');
                Ok(output)
            }
        }?;
        if output.len() > self.limits.max_output_bytes {
            return Err(serde_json::Error::io(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "rendered report exceeds the {}-byte output limit; use the compact profile or a narrower scope",
                    self.limits.max_output_bytes
                ),
            )));
        }
        Ok(output)
    }

    fn render_markdown(&self) -> String {
        let mut output = String::new();
        let command = match self.command.operation {
            Some(operation) => format!("{}: {}", self.command.name.label(), operation.label()),
            None => self.command.name.label().to_owned(),
        };

        writeln!(output, "# Codeplat {command}").expect("writing to a string cannot fail");
        writeln!(output).expect("writing to a string cannot fail");
        writeln!(output, "Schema version: {}", self.schema_version).expect("writing to a string cannot fail");
        writeln!(
            output,
            "Scope: `{}`",
            utils::escape_inline_code(&self.scope.selected_path)
        )
        .expect("writing to a string cannot fail");
        writeln!(output, "Status: {:?}", self.status).expect("writing to a string cannot fail");
        writeln!(output).expect("writing to a string cannot fail");
        writeln!(output, "## Summary").expect("writing to a string cannot fail");
        writeln!(output).expect("writing to a string cannot fail");
        writeln!(output, "{}", utils::sanitize_text(&self.summary)).expect("writing to a string cannot fail");

        if let Some(history) = &self.history {
            Render::history_markdown(&mut output, history);
        }

        if let Some(map) = &self.map {
            Render::map_markdown(&mut output, map);
        }

        if !self.findings.is_empty() {
            writeln!(output).expect("writing to a string cannot fail");
            writeln!(output, "## Findings").expect("writing to a string cannot fail");
            writeln!(output).expect("writing to a string cannot fail");
            for finding in &self.findings {
                writeln!(
                    output,
                    "- **{}:** {}",
                    utils::escape_markdown(&finding.title),
                    utils::sanitize_text(&finding.detail)
                )
                .expect("writing to a string cannot fail");
            }
        }

        if !self.limitations.is_empty() {
            writeln!(output).expect("writing to a string cannot fail");
            writeln!(output, "## Limitations").expect("writing to a string cannot fail");
            writeln!(output).expect("writing to a string cannot fail");
            for limitation in &self.limitations {
                writeln!(output, "- {}", utils::sanitize_text(&limitation.detail))
                    .expect("writing to a string cannot fail");
            }
        }

        output
    }
}

fn stable_repository_id(repository_root: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(repository_root.as_bytes());
    format!("sha256:{}", hex_digest(digest.finalize().as_slice()))
}

fn object_format(head: &HeadSnapshot) -> &'static str {
    match head.oid.as_deref().map(str::len) {
        Some(64) => "sha256",
        Some(40) => "sha1",
        _ => "unknown",
    }
}

fn hex_digest(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut output, "{byte:02x}").expect("writing a digest to a string cannot fail");
    }
    output
}

fn language_provenance(map: Option<&MapReport>) -> BTreeMap<String, LanguageProvenance> {
    let encountered = map.map(|report| &report.query_packs);
    map::language_capabilities()
        .into_iter()
        .filter(|capability| encountered.is_none_or(|packs| packs.contains_key(capability.language.label())))
        .map(|capability| {
            (
                capability.language.label().to_owned(),
                LanguageProvenance {
                    grammar: capability.grammar.to_owned(),
                    grammar_version: capability.grammar_version.to_owned(),
                    query_pack: capability.query_pack.to_owned(),
                    query_pack_version: capability.query_pack_version.to_owned(),
                },
            )
        })
        .collect()
}

fn report_quality(history: Option<&HistoryReport>, map: Option<&MapReport>) -> ReportQuality {
    let stale = map.is_some_and(|report| report.cache.status == CacheStatus::Stale || !report.cache.stale.is_empty());
    let map_truncated = map.is_some_and(|report| {
        [
            report.collections.files.truncated,
            report.collections.symbols.truncated,
            report.collections.omissions.truncated,
            report.collections.findings.truncated,
            report.collections.edges.truncated,
            report.collections.ranking.truncated,
            report.collections.snippets.truncated,
        ]
        .into_iter()
        .any(|value| value)
    });
    let history_truncated = history.is_some_and(|report| {
        [
            report.collections.commits.truncated,
            report.collections.churn_paths.truncated,
            report.collections.contributors_overall.truncated,
            report.collections.contributors_recent.truncated,
            report.collections.bug_paths.truncated,
            report.collections.bug_overlap_paths.truncated,
            report.collections.bug_commits.truncated,
            report.collections.activity_months.truncated,
            report.collections.firefighting_commits.truncated,
        ]
        .into_iter()
        .any(|value| value)
    });
    let incomplete =
        history.is_some_and(|report| report.provenance.completeness.status != HistoryCompletenessStatus::Complete);
    let unsupported = map.is_some_and(|report| {
        report
            .omissions
            .iter()
            .any(|omission| omission.reason == OmissionReason::UnsupportedLanguage)
    });
    let partial = map.is_some_and(|report| {
        report
            .files
            .iter()
            .any(|file| file.status == FileAnalysisStatus::Partial)
    });
    let mut strict_issues = Vec::new();
    if stale {
        strict_issues.push(StrictIssue::Stale);
    }
    if map_truncated || history_truncated {
        strict_issues.push(StrictIssue::Truncated);
    }
    if incomplete {
        strict_issues.push(StrictIssue::Incomplete);
    }
    if unsupported {
        strict_issues.push(StrictIssue::Unsupported);
    }
    if partial {
        strict_issues.push(StrictIssue::Partial);
    }
    ReportQuality {
        stale,
        truncated: map_truncated || history_truncated,
        incomplete,
        unsupported,
        partial,
        strict_issues,
    }
}

fn briefing_summary(history: &HistoryReport, map: &MapReport) -> String {
    format!(
        "Analyzed {} reachable commits ({} non-merge) and {} source files; ranked {} files within a {}-token source-map budget, with {} paths omitted in the selected scope.",
        history.commits_seen,
        history.non_merge_commits_seen,
        map.inventory.analyzed,
        map.ranking.len(),
        map.selection.token_budget,
        map.inventory.omitted,
    )
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HistoryReport {
    pub repository_root: String,
    pub scope_path: String,
    #[serde(default)]
    pub head: HeadSnapshot,
    #[serde(default)]
    pub provenance: HistoryProvenance,
    pub settings: HistorySettings,
    pub commits_seen: usize,
    pub non_merge_commits_seen: usize,
    #[serde(default)]
    pub collections: HistoryCollections,
    #[serde(default)]
    pub limitations: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub churn: Option<ChurnReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contributors: Option<ContributorReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bugs: Option<BugReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activity: Option<ActivityReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub firefighting: Option<FirefightingReport>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct HistoryCollections {
    pub commits: CollectionSummary,
    pub churn_paths: CollectionSummary,
    pub contributors_overall: CollectionSummary,
    pub contributors_recent: CollectionSummary,
    pub bug_paths: CollectionSummary,
    pub bug_overlap_paths: CollectionSummary,
    pub bug_commits: CollectionSummary,
    pub activity_months: CollectionSummary,
    pub firefighting_commits: CollectionSummary,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MapReport {
    #[serde(default)]
    pub profile: AnalysisProfile,
    pub repository_root: String,
    pub scope_path: String,
    #[serde(default)]
    pub head: HeadSnapshot,
    #[serde(default)]
    pub worktree: WorktreeSnapshot,
    pub query_pack: String,
    /// Query-pack provenance for every language variant encountered in this map.
    #[serde(default)]
    pub query_packs: BTreeMap<String, String>,
    pub exclusions: Vec<String>,
    pub inventory: MapInventory,
    pub files: Vec<SourceFile>,
    pub omissions: Vec<SourceOmission>,
    pub findings: Vec<MapFinding>,
    pub limitations: Vec<String>,
    pub edges: Vec<LexicalEdge>,
    pub ranking: Vec<FileRank>,
    pub selection: MapSelection,
    pub cache: MapCacheReport,
    #[serde(default)]
    pub collections: MapCollections,
}

impl MapReport {
    pub fn compact_payload_tokens(&self) -> usize {
        let mut tokens = 0usize;
        for file in &self.files {
            tokens = tokens.saturating_add(token_count(&file.path));
            tokens = tokens
                .saturating_add(token_count(file.language.label()))
                .saturating_add(token_count(&file.extension))
                .saturating_add(token_count(file.worktree_state.label()))
                .saturating_add(token_count(file.status.label()));
            tokens = tokens.saturating_add(
                file.limitations
                    .iter()
                    .map(|limitation| token_count(limitation))
                    .sum::<usize>(),
            );
            for symbol in &file.symbols {
                tokens = tokens.saturating_add(token_count(&symbol.name));
                tokens = tokens.saturating_add(token_count(&symbol.context));
                tokens = tokens.saturating_add(token_count(symbol.kind.label()));
                tokens = tokens.saturating_add(token_count(symbol.role.label()));
                tokens = tokens.saturating_add(symbol.scope.iter().map(|scope| token_count(scope)).sum::<usize>());
                tokens = tokens.saturating_add(8);
            }
        }
        for omission in &self.omissions {
            tokens = tokens.saturating_add(token_count(&omission.path));
            tokens = tokens.saturating_add(token_count(&omission.detail));
        }
        for finding in &self.findings {
            tokens = tokens.saturating_add(token_count(finding.kind.label()));
            tokens = tokens.saturating_add(token_count(&finding.path));
            tokens = tokens.saturating_add(token_count(&finding.detail));
            tokens = tokens.saturating_add(if finding.location.is_some() { 8 } else { 0 });
        }
        for edge in &self.edges {
            tokens = tokens.saturating_add(token_count(&edge.source));
            tokens = tokens.saturating_add(token_count(&edge.target));
            tokens = tokens.saturating_add(token_count(&edge.symbol));
            tokens = tokens.saturating_add(
                edge.candidates
                    .iter()
                    .map(|candidate| token_count(candidate))
                    .sum::<usize>(),
            );
            tokens = tokens.saturating_add(if edge.ambiguous { 1 } else { 0 });
        }
        tokens = tokens.saturating_add(
            self.ranking
                .iter()
                .map(|rank| token_count(&rank.path).saturating_add(2))
                .sum::<usize>(),
        );
        tokens.saturating_add(
            self.selection
                .snippets
                .iter()
                .map(|snippet| snippet.estimated_tokens)
                .sum::<usize>(),
        )
    }

    fn summary(&self) -> String {
        let mut languages = self
            .files
            .iter()
            .map(|file| file.language.display_label())
            .collect::<Vec<_>>();
        languages.sort_unstable();
        languages.dedup();
        if languages.is_empty() || (languages.len() == 1 && languages[0] == SourceLanguage::Rust.display_label()) {
            format!(
                "Analyzed {} Rust source files and recorded {} omitted paths within the selected source scope.",
                self.inventory.analyzed, self.inventory.omitted
            )
        } else {
            format!(
                "Analyzed {} source files ({}) and recorded {} omitted paths within the selected source scope.",
                self.inventory.analyzed,
                languages.join(", "),
                self.inventory.omitted
            )
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LanguageCapability {
    pub language: SourceLanguage,
    pub extensions: Vec<String>,
    pub grammar: String,
    pub grammar_version: String,
    pub query_pack: String,
    pub query_pack_version: String,
    pub definitions: bool,
    pub references: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct MapCollections {
    pub files: CollectionSummary,
    pub symbols: CollectionSummary,
    pub omissions: CollectionSummary,
    pub findings: CollectionSummary,
    pub edges: CollectionSummary,
    pub ranking: CollectionSummary,
    pub snippets: CollectionSummary,
}

/// One file-level lexical dependency candidate. This is intentionally not a
/// type-resolved call graph edge.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LexicalEdge {
    pub source: String,
    pub target: String,
    pub symbol: String,
    pub ambiguous: bool,
    /// All lexical definition candidates for this reference.
    pub candidates: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FileRank {
    pub path: String,
    /// PageRank plus explicit-focus score scaled by 1,000,000.
    pub score: u64,
    pub focus_matches: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MapSelection {
    pub token_budget: usize,
    pub estimated_tokens: usize,
    pub snippets: Vec<MapSnippet>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MapSnippet {
    pub path: String,
    pub language: SourceLanguage,
    pub symbol: SourceSymbol,
    /// Symbol score scaled by 1,000,000.
    pub score: u64,
    pub estimated_tokens: usize,
    pub truncated: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MapCacheReport {
    pub mode: CacheMode,
    pub status: CacheStatus,
    /// Number of normalized `--cache-file` names that matched eligible paths.
    #[serde(default)]
    pub matched: usize,
    /// Number of normalized `--cache-file` names that matched no eligible path.
    #[serde(default)]
    pub unmatched: usize,
    /// Number of eligible paths that could not be analyzed because this mode
    /// did not permit a cache refresh and no usable record was available.
    #[serde(default)]
    pub unavailable: usize,
    pub hits: usize,
    pub misses: usize,
    pub refreshed: Vec<String>,
    pub stale: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MapInventory {
    pub tracked: usize,
    pub modified: usize,
    pub untracked: usize,
    pub analyzed: usize,
    pub omitted: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SourceFile {
    pub path: String,
    pub language: SourceLanguage,
    pub extension: String,
    pub worktree_state: WorktreeState,
    pub status: FileAnalysisStatus,
    pub symbols: Vec<SourceSymbol>,
    pub limitations: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SourceSymbol {
    pub name: String,
    pub kind: SymbolKind,
    pub role: SymbolRole,
    pub scope: Vec<String>,
    pub location: SourceLocation,
    pub context: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SourceLocation {
    pub start: Position,
    pub end: Position,
}

impl From<tree_sitter::Node<'_>> for SourceLocation {
    fn from(node: tree_sitter::Node) -> Self {
        let start = node.start_position();
        let end = node.end_position();
        SourceLocation {
            start: Position { line: start.row + 1, column: start.column + 1 },
            end: Position { line: end.row + 1, column: end.column + 1 },
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SourceOmission {
    pub path: String,
    pub reason: OmissionReason,
    pub detail: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MapFinding {
    pub kind: MapFindingKind,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<SourceLocation>,
    pub detail: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ChurnReport {
    pub window_days: u32,
    pub paths: Vec<PathCount>,
    pub caveats: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContributorReport {
    pub recent_window_days: u32,
    pub overall: Vec<ContributorCount>,
    pub recent: Vec<ContributorCount>,
    pub caveats: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContributorCount {
    pub name: String,
    pub email: String,
    pub commits: usize,
    pub share_percent: u8,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BugReport {
    pub window_days: u32,
    pub keywords: Vec<String>,
    pub paths: Vec<PathCount>,
    pub overlap_paths: Vec<PathCount>,
    pub commits: Vec<CommitEvidence>,
    pub caveats: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ActivityReport {
    pub months: Vec<MonthlyActivity>,
    pub caveats: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MonthlyActivity {
    pub month: String,
    pub commits: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FirefightingReport {
    pub window_days: u32,
    pub keywords: Vec<String>,
    pub commits: Vec<CommitEvidence>,
    pub caveats: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CommitEvidence {
    pub id: String,
    pub subject: String,
    pub paths: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PathCount {
    pub path: String,
    pub commits: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CommandDescriptor {
    pub name: CommandName,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation: Option<HistoryOperation>,
    #[serde(skip)]
    pub path: PathBuf,
}

impl CommandDescriptor {
    pub fn briefing(path: PathBuf) -> Self {
        Self { name: CommandName::Briefing, operation: None, path }
    }

    pub fn map(path: PathBuf) -> Self {
        Self { name: CommandName::Map, operation: None, path }
    }

    pub fn history(path: PathBuf, operation: Option<HistoryOperation>) -> Self {
        Self { name: CommandName::History, operation, path }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReportScope {
    pub selected_path: String,
}

impl From<PathBuf> for ReportScope {
    fn from(path: PathBuf) -> Self {
        Self { selected_path: path.to_string_lossy().into_owned() }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Finding {
    pub title: String,
    pub detail: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Limitation {
    pub detail: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CapabilitiesReport {
    pub schema_version: u16,
    pub report_kind: String,
    pub tool_version: String,
    pub languages: Vec<LanguageCapability>,
    pub query_packs_valid: bool,
    pub limits: BTreeMap<String, ReportLimits>,
}

impl CapabilitiesReport {
    pub fn current() -> Self {
        let limits = [
            (
                "compact".to_owned(),
                ReportLimits::for_profile(AnalysisProfile::Compact),
            ),
            (
                "evidence".to_owned(),
                ReportLimits::for_profile(AnalysisProfile::Evidence),
            ),
        ]
        .into_iter()
        .collect();
        Self {
            schema_version: SCHEMA_VERSION,
            report_kind: "capabilities".to_owned(),
            tool_version: TOOL_VERSION.to_owned(),
            languages: map::language_capabilities(),
            query_packs_valid: map::validate_query_packs().is_ok(),
            limits,
        }
    }

    pub fn render(&self, format: OutputFormat) -> Result<String, serde_json::Error> {
        match format {
            OutputFormat::Json => {
                let mut output = serde_json::to_string_pretty(self)?;
                output.push('\n');
                Ok(output)
            }
            OutputFormat::Markdown => {
                let mut output = String::from("# Codeplat capabilities\n\n");
                writeln!(output, "Schema version: {}", self.schema_version)
                    .expect("writing capabilities to a string cannot fail");
                writeln!(output, "Tool version: {}", self.tool_version)
                    .expect("writing capabilities to a string cannot fail");
                writeln!(output, "Query packs valid: {}", self.query_packs_valid)
                    .expect("writing capabilities to a string cannot fail");
                writeln!(output, "\n## Languages\n").expect("writing capabilities to a string cannot fail");
                for language in &self.languages {
                    writeln!(
                        output,
                        "- {} (`{}`) — grammar {} {}, query pack {} {}",
                        language.language.display_label(),
                        language.extensions.join(", "),
                        language.grammar,
                        language.grammar_version,
                        language.query_pack,
                        language.query_pack_version
                    )
                    .expect("writing capabilities to a string cannot fail");
                }
                Ok(output)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DoctorCheckStatus {
    Pass,
    Warn,
    Fail,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DoctorCheck {
    pub name: String,
    pub status: DoctorCheckStatus,
    pub detail: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DoctorReport {
    pub schema_version: u16,
    pub report_kind: String,
    pub tool_version: String,
    pub requested_path: String,
    pub repository: Option<RepositoryIdentity>,
    pub checks: Vec<DoctorCheck>,
    pub limits: BTreeMap<String, ReportLimits>,
    pub source_evidence_collected: bool,
    pub repository_state_changed: bool,
}

impl DoctorReport {
    pub fn run(path: PathBuf) -> Self {
        let requested_path = path.to_string_lossy().into_owned();
        let mut checks = Vec::new();
        let mut repository = None;

        let absolute = match security::absolute_input_path(&path) {
            Ok(path) => {
                checks.push(DoctorCheck {
                    name: "input_path".to_owned(),
                    status: DoctorCheckStatus::Pass,
                    detail: format!("resolved to `{}`", path.display()),
                });
                path
            }
            Err(error) => {
                checks.push(DoctorCheck {
                    name: "input_path".to_owned(),
                    status: DoctorCheckStatus::Fail,
                    detail: error.to_string(),
                });
                return Self::with_checks(requested_path, repository, checks);
            }
        };

        let safety_detail = if cfg!(unix) {
            "Unix descriptor-relative no-follow reads and atomic cache writes are available."
        } else {
            "Component reparse checks and standard-library fallback are available; concurrent rename races are weaker on this target."
        };
        checks.push(DoctorCheck {
            name: "path_safety".to_owned(),
            status: DoctorCheckStatus::Pass,
            detail: safety_detail.to_owned(),
        });

        match security::discover_repository(&absolute) {
            Ok(repo) => match security::resolve_scope(&repo, &absolute) {
                Ok(scope) => {
                    let root = scope.repository_root.to_string_lossy().into_owned();
                    let stable_id = stable_repository_id(&root);
                    repository = Some(RepositoryIdentity {
                        canonical_root: root,
                        stable_id,
                        object_format: repo.object_hash().to_string(),
                    });
                    checks.push(DoctorCheck {
                        name: "repository_discovery".to_owned(),
                        status: DoctorCheckStatus::Pass,
                        detail: "repository and selected scope resolved without source analysis".to_owned(),
                    });
                }
                Err(error) => checks.push(DoctorCheck {
                    name: "repository_scope".to_owned(),
                    status: DoctorCheckStatus::Fail,
                    detail: error.to_string(),
                }),
            },
            Err(error) => checks.push(DoctorCheck {
                name: "repository_discovery".to_owned(),
                status: DoctorCheckStatus::Fail,
                detail: error.to_string(),
            }),
        }

        checks.push(
            match serde_json::from_str::<serde_json::Value>(include_str!("../schema/v1/codeplat.json")) {
                Ok(_) => DoctorCheck {
                    name: "schema".to_owned(),
                    status: DoctorCheckStatus::Pass,
                    detail: format!("{} is embedded and valid JSON", SCHEMA_PATH),
                },
                Err(error) => DoctorCheck {
                    name: "schema".to_owned(),
                    status: DoctorCheckStatus::Fail,
                    detail: error.to_string(),
                },
            },
        );
        checks.push(match map::validate_query_packs() {
            Ok(()) => DoctorCheck {
                name: "query_packs".to_owned(),
                status: DoctorCheckStatus::Pass,
                detail: "all compiled grammars and definition/reference query packs are available".to_owned(),
            },
            Err(error) => {
                DoctorCheck { name: "query_packs".to_owned(), status: DoctorCheckStatus::Fail, detail: error }
            }
        });

        checks.push(cache_doctor_check());
        Self::with_checks(requested_path, repository, checks)
    }

    fn with_checks(requested_path: String, repository: Option<RepositoryIdentity>, checks: Vec<DoctorCheck>) -> Self {
        let limits = [
            (
                "compact".to_owned(),
                ReportLimits::for_profile(AnalysisProfile::Compact),
            ),
            (
                "evidence".to_owned(),
                ReportLimits::for_profile(AnalysisProfile::Evidence),
            ),
        ]
        .into_iter()
        .collect();
        Self {
            schema_version: SCHEMA_VERSION,
            report_kind: "doctor".to_owned(),
            tool_version: TOOL_VERSION.to_owned(),
            requested_path,
            repository,
            checks,
            limits,
            source_evidence_collected: false,
            repository_state_changed: false,
        }
    }

    pub fn is_ok(&self) -> bool {
        self.checks.iter().all(|check| check.status != DoctorCheckStatus::Fail)
    }

    pub fn render(&self, format: OutputFormat) -> Result<String, serde_json::Error> {
        match format {
            OutputFormat::Json => {
                let mut output = serde_json::to_string_pretty(self)?;
                output.push('\n');
                Ok(output)
            }
            OutputFormat::Markdown => {
                let mut output = String::from("# Codeplat doctor\n\n");
                writeln!(output, "Tool version: {}", self.tool_version)
                    .expect("writing doctor output to a string cannot fail");
                writeln!(output, "Source evidence collected: {}", self.source_evidence_collected)
                    .expect("writing doctor output to a string cannot fail");
                writeln!(output, "Repository state changed: {}\n", self.repository_state_changed)
                    .expect("writing doctor output to a string cannot fail");
                for check in &self.checks {
                    writeln!(output, "- **{:?}** {}: {}", check.status, check.name, check.detail)
                        .expect("writing doctor output to a string cannot fail");
                }
                Ok(output)
            }
        }
    }
}

fn cache_doctor_check() -> DoctorCheck {
    match security::configured_cache_root() {
        Ok(Some(path)) => {
            let metadata = fs::metadata(&path);
            let (status, detail) = match metadata {
                Ok(metadata) if !metadata.is_dir() => (
                    DoctorCheckStatus::Fail,
                    format!("cache path `{}` is not a directory", path.display()),
                ),
                Ok(metadata) => {
                    #[cfg(unix)]
                    let private = {
                        use std::os::unix::fs::PermissionsExt;
                        metadata.permissions().mode() & 0o077 == 0
                    };
                    #[cfg(not(unix))]
                    let private = true;
                    if private {
                        (
                            DoctorCheckStatus::Pass,
                            format!("cache directory `{}` is private", path.display()),
                        )
                    } else {
                        (
                            DoctorCheckStatus::Fail,
                            format!("cache directory `{}` is group/world accessible", path.display()),
                        )
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => (
                    DoctorCheckStatus::Warn,
                    format!("cache directory `{}` will be created on first use", path.display()),
                ),
                Err(error) => (
                    DoctorCheckStatus::Fail,
                    format!("could not inspect cache path: {error}"),
                ),
            };
            DoctorCheck { name: "cache".to_owned(), status, detail }
        }
        Ok(None) => DoctorCheck {
            name: "cache".to_owned(),
            status: DoctorCheckStatus::Fail,
            detail: "neither XDG_CACHE_HOME nor HOME provided a usable cache location".to_owned(),
        },
        Err(error) => {
            DoctorCheck { name: "cache".to_owned(), status: DoctorCheckStatus::Fail, detail: error.to_string() }
        }
    }
}

struct Render;

impl Render {
    fn commits(output: &mut String, commits: &[CommitEvidence]) {
        writeln!(output, "#### Evidence commits").expect("writing to a string cannot fail");
        if commits.is_empty() {
            writeln!(output, "No matching commits were found.").expect("writing to a string cannot fail");
        } else {
            for commit in commits {
                let paths =
                    if commit.paths.is_empty() { "no in-scope paths".to_owned() } else { commit.paths.join(", ") };
                writeln!(
                    output,
                    "- `{}` — {} ({})",
                    utils::escape_inline_code(&commit.id),
                    utils::sanitize_text(&commit.subject),
                    utils::sanitize_text(&paths)
                )
                .expect("writing to a string cannot fail");
            }
        }
    }

    fn section_heading(output: &mut String, heading: &str) {
        writeln!(output).expect("writing to a string cannot fail");
        writeln!(output, "### {heading}").expect("writing to a string cannot fail");
        writeln!(output).expect("writing to a string cannot fail");
    }

    fn caveats(output: &mut String, caveats: &[String]) {
        if caveats.is_empty() {
            return;
        }
        writeln!(output, "Caveats:").expect("writing to a string cannot fail");
        for caveat in caveats {
            writeln!(output, "- {}", utils::sanitize_text(caveat)).expect("writing to a string cannot fail");
        }
    }

    fn history_markdown(output: &mut String, history: &HistoryReport) {
        writeln!(output).expect("writing to a string cannot fail");
        writeln!(output, "## History analysis").expect("writing to a string cannot fail");
        writeln!(output).expect("writing to a string cannot fail");
        writeln!(
            output,
            "Repository: `{}`",
            utils::escape_inline_code(&history.repository_root)
        )
        .expect("writing to a string cannot fail");
        writeln!(
            output,
            "History scope: `{}`",
            utils::escape_inline_code(&history.scope_path)
        )
        .expect("writing to a string cannot fail");
        writeln!(output, "Reachable commits: {}", history.commits_seen).expect("writing to a string cannot fail");
        writeln!(output, "Non-merge commits: {}", history.non_merge_commits_seen)
            .expect("writing to a string cannot fail");
        writeln!(
            output,
            "Windows: {} days for churn/bugs/firefighting; {} days for recent contributors",
            history.settings.window_days, history.settings.recent_window_days
        )
        .expect("writing to a string cannot fail");
        writeln!(
            output,
            "Bug keywords: `{}`",
            utils::escape_inline_code(&history.settings.bug_keywords.join("`, `"))
        )
        .expect("writing to a string cannot fail");
        writeln!(
            output,
            "Firefighting keywords: `{}`",
            utils::escape_inline_code(&history.settings.firefighting_keywords.join("`, `"))
        )
        .expect("writing to a string cannot fail");
        if history.collections.commits.truncated
            || history.collections.churn_paths.truncated
            || history.collections.contributors_overall.truncated
            || history.collections.contributors_recent.truncated
            || history.collections.bug_paths.truncated
            || history.collections.bug_overlap_paths.truncated
            || history.collections.bug_commits.truncated
            || history.collections.activity_months.truncated
            || history.collections.firefighting_commits.truncated
        {
            writeln!(
                output,
                "Evidence collections are bounded; JSON contains totals and truncation reasons."
            )
            .expect("writing to a string cannot fail");
        }

        if let Some(churn) = &history.churn {
            Render::churn_markdown(output, churn);
        }
        if let Some(contributors) = &history.contributors {
            Render::contributors_markdown(output, contributors);
        }
        if let Some(bugs) = &history.bugs {
            Render::bugs_markdown(output, bugs);
        }
        if let Some(activity) = &history.activity {
            Render::activity_markdown(output, activity);
        }
        if let Some(firefighting) = &history.firefighting {
            Render::firefighting_markdown(output, firefighting);
        }
        for limitation in &history.limitations {
            writeln!(output, "- Limitation: {}", utils::sanitize_text(limitation))
                .expect("writing to a string cannot fail");
        }
    }

    fn map_markdown(output: &mut String, map: &MapReport) {
        writeln!(output).expect("writing to a string cannot fail");
        writeln!(output, "## Source map").expect("writing to a string cannot fail");
        writeln!(output).expect("writing to a string cannot fail");
        writeln!(
            output,
            "Repository: `{}`",
            utils::escape_inline_code(&map.repository_root)
        )
        .expect("writing to a string cannot fail");
        writeln!(output, "Map scope: `{}`", utils::escape_inline_code(&map.scope_path))
            .expect("writing to a string cannot fail");
        writeln!(output, "Query pack: `{}`", utils::escape_inline_code(&map.query_pack))
            .expect("writing to a string cannot fail");
        if map.query_packs.len() > 1 {
            let provenance = map
                .query_packs
                .iter()
                .map(|(language, query_pack)| format!("{language}={query_pack}"))
                .collect::<Vec<_>>()
                .join(", ");
            writeln!(output, "Query packs: `{}`", utils::escape_inline_code(&provenance))
                .expect("writing to a string cannot fail");
        }
        writeln!(
            output,
            "Inventory: {} tracked ({} modified), {} untracked, {} analyzed, {} omitted",
            map.inventory.tracked,
            map.inventory.modified,
            map.inventory.untracked,
            map.inventory.analyzed,
            map.inventory.omitted
        )
        .expect("writing to a string cannot fail");
        if map.collections.files.truncated
            || map.collections.symbols.truncated
            || map.collections.omissions.truncated
            || map.collections.findings.truncated
            || map.collections.edges.truncated
            || map.collections.ranking.truncated
            || map.collections.snippets.truncated
        {
            writeln!(
                output,
                "Collections are bounded; JSON contains totals and truncation reasons."
            )
            .expect("writing to a string cannot fail");
        }
        if !map.exclusions.is_empty() {
            writeln!(
                output,
                "Exclusions: `{}`",
                utils::escape_inline_code(&map.exclusions.join("`, `"))
            )
            .expect("writing to a string cannot fail");
        }

        if !map.files.is_empty()
            || map.cache.matched > 0
            || map.cache.unmatched > 0
            || map.cache.unavailable > 0
            || map.cache.hits > 0
            || map.cache.misses > 0
            || !map.cache.refreshed.is_empty()
            || !map.cache.stale.is_empty()
        {
            writeln!(
                output,
                "Cache: {} ({}) — {} matched, {} unmatched, {} unavailable, {} hits, {} misses, {} refreshed, {} stale",
                map.cache.mode.label(),
                map.cache.status.label(),
                map.cache.matched,
                map.cache.unmatched,
                map.cache.unavailable,
                map.cache.hits,
                map.cache.misses,
                map.cache.refreshed.len(),
                map.cache.stale.len()
            )
            .expect("writing to a string cannot fail");
            if !map.files.is_empty() {
                writeln!(
                    output,
                    "Ranking: {} files; map budget {} tokens, selected {}",
                    map.ranking.len(),
                    map.selection.token_budget,
                    map.selection.estimated_tokens
                )
                .expect("writing to a string cannot fail");
                Render::section_heading(output, "Ranked map selection");
                if map.selection.snippets.is_empty() {
                    writeln!(output, "No structural snippets fit the map token budget.")
                        .expect("writing to a string cannot fail");
                } else {
                    for snippet in &map.selection.snippets {
                        let location = Self::format_location(&snippet.symbol.location);
                        let scope = if snippet.symbol.scope.is_empty() {
                            "root".to_owned()
                        } else {
                            snippet.symbol.scope.join("::")
                        };
                        writeln!(
                            output,
                            "- `{}` — {} `{}` at {} in `{}` (score {}, {} tokens) — `{}`{}",
                            utils::escape_inline_code(&snippet.path),
                            snippet.symbol.kind.label(),
                            utils::escape_inline_code(&snippet.symbol.name),
                            location,
                            utils::escape_inline_code(&scope),
                            snippet.score,
                            snippet.estimated_tokens,
                            utils::escape_inline_code(&snippet.symbol.context),
                            if snippet.truncated { " (elided)" } else { "" }
                        )
                        .expect("writing to a string cannot fail");
                    }
                }
            }
        }

        let mut files_by_language: BTreeMap<SourceLanguage, Vec<&SourceFile>> = BTreeMap::new();
        for file in &map.files {
            files_by_language.entry(file.language).or_default().push(file);
        }
        if files_by_language.len() <= 1 {
            if map.files.is_empty() {
                Render::section_heading(output, "Rust files");
                writeln!(output, "No Rust files were analyzed.").expect("writing to a string cannot fail");
            } else {
                let (language, files) = files_by_language.iter().next().expect("one language group");
                Render::section_heading(output, &format!("{} files", language.display_label()));
                Render::source_files(output, files);
            }
        } else {
            for (language, files) in &files_by_language {
                Render::section_heading(output, &format!("{} files", language.display_label()));
                Render::source_files(output, files);
            }
        }
        if !map.findings.is_empty() {
            Render::section_heading(output, "Map findings");
            for finding in &map.findings {
                let location = finding
                    .location
                    .as_ref()
                    .map(Self::format_location)
                    .unwrap_or_else(|| "unknown location".to_owned());
                writeln!(
                    output,
                    "- **{}** `{}`{} — {}",
                    finding.kind.label(),
                    utils::escape_inline_code(&finding.path),
                    if finding.location.is_some() { format!(" at {location}") } else { String::new() },
                    utils::sanitize_text(&finding.detail)
                )
                .expect("writing to a string cannot fail");
            }
        }

        if !map.edges.is_empty() {
            Render::section_heading(output, "Lexical dependency edges");
            for edge in &map.edges {
                writeln!(
                    output,
                    "- `{}` → `{}` via `{}`{}",
                    utils::escape_inline_code(&edge.source),
                    utils::escape_inline_code(&edge.target),
                    utils::escape_inline_code(&edge.symbol),
                    if edge.ambiguous { " (ambiguous candidate)" } else { "" }
                )
                .expect("writing to a string cannot fail");
            }
        }

        if !map.omissions.is_empty() {
            Render::section_heading(output, "Omitted paths");
            for omission in &map.omissions {
                writeln!(
                    output,
                    "- `{}` — **{}:** {}",
                    utils::escape_inline_code(&omission.path),
                    omission.reason.label(),
                    utils::sanitize_text(&omission.detail)
                )
                .expect("writing to a string cannot fail");
            }
        }

        Render::section_heading(output, "Map limitations");
        for limitation in &map.limitations {
            writeln!(output, "- {}", utils::sanitize_text(limitation)).expect("writing to a string cannot fail");
        }
    }

    fn source_files(output: &mut String, files: &[&SourceFile]) {
        for file in files {
            writeln!(
                output,
                "- `{}` — {} (.{}), {} {}, {} symbols",
                utils::escape_inline_code(&file.path),
                file.language.display_label(),
                file.extension,
                file.worktree_state.label(),
                file.status.label(),
                file.symbols.len()
            )
            .expect("writing to a string cannot fail");
            writeln!(
                output,
                "  - Structural snippets are shown in the ranked selection above."
            )
            .expect("writing to a string cannot fail");
            for limitation in &file.limitations {
                writeln!(output, "  - Limitation: {}", utils::sanitize_text(limitation))
                    .expect("writing to a string cannot fail");
            }
        }
    }

    fn format_location(location: &SourceLocation) -> String {
        format!(
            "{}:{}-{}:{}",
            location.start.line, location.start.column, location.end.line, location.end.column
        )
    }

    fn churn_markdown(output: &mut String, churn: &ChurnReport) {
        Render::section_heading(output, "Churn hotspots");
        writeln!(output, "Window: {} days", churn.window_days).expect("writing to a string cannot fail");
        if churn.paths.is_empty() {
            writeln!(output, "No in-scope non-merge paths changed in this window.")
                .expect("writing to a string cannot fail");
        } else {
            for path in &churn.paths {
                writeln!(
                    output,
                    "- `{}` — {} commits",
                    utils::escape_inline_code(&path.path),
                    path.commits
                )
                .expect("writing to a string cannot fail");
            }
        }
        Render::caveats(output, &churn.caveats);
    }

    fn contributors_markdown(output: &mut String, contributors: &ContributorReport) {
        Render::section_heading(output, "Contributor concentration");
        Render::contributors_group(output, "All non-merge commits", &contributors.overall);
        Render::contributors_group(output, "Recent non-merge commits", &contributors.recent);
        Render::caveats(output, &contributors.caveats);
    }

    fn contributors_group(output: &mut String, label: &str, contributors: &[ContributorCount]) {
        writeln!(output, "#### {label}").expect("writing to a string cannot fail");
        if contributors.is_empty() {
            writeln!(output, "No contributors were found.").expect("writing to a string cannot fail");
            return;
        }
        for contributor in contributors {
            writeln!(
                output,
                "- {} <{}> — {} commits ({}%)",
                utils::sanitize_text(&contributor.name),
                utils::sanitize_text(&contributor.email),
                contributor.commits,
                contributor.share_percent
            )
            .expect("writing to a string cannot fail");
        }
    }

    fn bugs_markdown(output: &mut String, bugs: &BugReport) {
        Render::section_heading(output, "Bug-related clusters");
        writeln!(output, "Window: {} days", bugs.window_days).expect("writing to a string cannot fail");
        writeln!(
            output,
            "Keywords: `{}`",
            utils::escape_inline_code(&bugs.keywords.join("`, `"))
        )
        .expect("writing to a string cannot fail");
        Render::paths(output, "Bug-related paths", &bugs.paths);
        Render::paths(output, "Churn overlap", &bugs.overlap_paths);
        Render::commits(output, &bugs.commits);
        Render::caveats(output, &bugs.caveats);
    }

    fn activity_markdown(output: &mut String, activity: &ActivityReport) {
        Render::section_heading(output, "Monthly activity");
        if activity.months.is_empty() {
            writeln!(output, "No commits were found.").expect("writing to a string cannot fail");
        } else {
            for month in &activity.months {
                writeln!(output, "- {} — {} commits", month.month, month.commits)
                    .expect("writing to a string cannot fail");
            }
        }
        Render::caveats(output, &activity.caveats);
    }

    fn firefighting_markdown(output: &mut String, firefighting: &FirefightingReport) {
        Render::section_heading(output, "Firefighting commits");
        writeln!(
            output,
            "Window: {} days; keywords: `{}`",
            firefighting.window_days,
            utils::escape_inline_code(&firefighting.keywords.join("`, `"))
        )
        .expect("writing to a string cannot fail");
        Render::commits(output, &firefighting.commits);
        Render::caveats(output, &firefighting.caveats);
    }

    fn paths(output: &mut String, label: &str, paths: &[PathCount]) {
        writeln!(output, "#### {label}").expect("writing to a string cannot fail");
        if paths.is_empty() {
            writeln!(output, "No paths were found.").expect("writing to a string cannot fail");
        } else {
            for path in paths {
                writeln!(
                    output,
                    "- `{}` — {} commits",
                    utils::escape_inline_code(&path.path),
                    path.commits
                )
                .expect("writing to a string cannot fail");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::{CommandDescriptor, Finding, Limitation, ReportScope, ReportStatus, SCHEMA_VERSION};
    use std::path::PathBuf;

    #[test]
    fn markdown_escapes_report_content_that_could_add_control_sequences() {
        let report = Report {
            schema_version: SCHEMA_VERSION,
            profile: AnalysisProfile::Compact,
            limits: ReportLimits::for_profile(AnalysisProfile::Compact),
            command: CommandDescriptor::map(PathBuf::from("unsafe\u{1b}[31m-path")),
            scope: ReportScope { selected_path: "unsafe\u{1b}[31m-path".to_owned() },
            status: ReportStatus::Foundation,
            summary: "A\u{1b}[31m summary".to_owned(),
            provenance: ReportProvenance::default(),
            quality: ReportQuality::default(),
            findings: vec![Finding { title: "title*".to_owned(), detail: "detail\u{7}".to_owned() }],
            limitations: vec![Limitation { detail: "limitation\u{1b}[0m".to_owned() }],
            history: None,
            map: None,
        };

        let markdown = report.render(OutputFormat::Markdown).expect("markdown renders");
        assert!(!markdown.contains('\u{1b}'));
        assert!(!markdown.contains('\u{7}'));
        assert!(markdown.contains("title\\*"));
    }

    #[test]
    fn schema_and_golden_v1_corpus_cover_all_report_variants() {
        let schema: serde_json::Value =
            serde_json::from_str(include_str!("../schema/v1/codeplat.json")).expect("schema is valid JSON");
        assert_eq!(
            schema["$defs"]["analysis_report"]["properties"]["schema_version"]["const"],
            1
        );
        assert!(
            schema["$defs"]["analysis_report"]["required"]
                .as_array()
                .expect("analysis required fields")
                .iter()
                .any(|field| field == "command")
        );

        let analysis = [
            include_str!("../schema/v1/golden/briefing.json"),
            include_str!("../schema/v1/golden/map.json"),
            include_str!("../schema/v1/golden/history.json"),
        ];
        for document in analysis {
            let report: Report = serde_json::from_str(document).expect("historical v1 report remains readable");
            assert_eq!(report.schema_version, SCHEMA_VERSION);
        }
        let capabilities: CapabilitiesReport =
            serde_json::from_str(include_str!("../schema/v1/golden/capabilities.json"))
                .expect("capabilities golden remains readable");
        assert_eq!(capabilities.schema_version, SCHEMA_VERSION);
        let doctor: DoctorReport = serde_json::from_str(include_str!("../schema/v1/golden/doctor.json"))
            .expect("doctor golden remains readable");
        assert_eq!(doctor.schema_version, SCHEMA_VERSION);
        assert!(!doctor.source_evidence_collected);
        assert!(!doctor.repository_state_changed);
    }
}
