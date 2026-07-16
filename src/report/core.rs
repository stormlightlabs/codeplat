use super::*;

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
    pub max_landmarks: usize,
    pub max_project_roots: usize,
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
            max_landmarks: match profile {
                AnalysisProfile::Compact => 64,
                AnalysisProfile::Evidence => 4_096,
            },
            max_project_roots: match profile {
                AnalysisProfile::Compact => 32,
                AnalysisProfile::Evidence => 1_024,
            },
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
    Explain,
}

impl CommandName {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Briefing => "briefing",
            Self::Map => "map",
            Self::History => "history",
            Self::Explain => "explain",
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
    Unknown,
}

impl WorktreeState {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Tracked => "tracked",
            Self::Modified => "modified",
            Self::Untracked => "untracked",
            Self::Unknown => "unknown",
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

/// Visibility inferred from declaration modifiers. This is ranking evidence,
/// not a language-semantic access check.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolVisibility {
    Public,
    Private,
    Internal,
    #[default]
    Unknown,
}

impl SymbolVisibility {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Private => "private",
            Self::Internal => "internal",
            Self::Unknown => "unknown",
        }
    }
}

/// Syntactic evidence attached to a raw Tree-sitter tag. Bare references
/// remain available as evidence but never create graph edges by themselves.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolEvidence {
    #[default]
    Declaration,
    Import,
    Call,
    TypeReference,
    MemberReference,
    BareReference,
}

impl SymbolEvidence {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Declaration => "declaration",
            Self::Import => "import",
            Self::Call => "call",
            Self::TypeReference => "type_reference",
            Self::MemberReference => "member_reference",
            Self::BareReference => "bare_reference",
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
    NonSource,
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
            Self::NonSource => "non_source",
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

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LexicalResolutionReason {
    #[default]
    SameFileExplicit,
    ImportedModule,
    ImportedName,
}

impl LexicalResolutionReason {
    pub const fn label(self) -> &'static str {
        match self {
            Self::SameFileExplicit => "same_file_explicit",
            Self::ImportedModule => "imported_module",
            Self::ImportedName => "imported_name",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceTier {
    High,
    #[default]
    Medium,
    Low,
}

impl ConfidenceTier {
    pub const fn label(self) -> &'static str {
        match self {
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
        }
    }
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

impl WorktreeSnapshotState {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Clean => "clean",
            Self::Modified => "modified",
            Self::Untracked => "untracked",
            Self::Mixed => "mixed",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeywordMatchMode {
    #[default]
    Word,
    Substring,
}

impl KeywordMatchMode {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Word => "word",
            Self::Substring => "substring",
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

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryCompletenessStatus {
    #[default]
    Complete,
    Shallow,
    MissingObjects,
    Partial,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DoctorCheckStatus {
    Pass,
    Warn,
    Fail,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExplainTargetKind {
    Path,
    Symbol,
    Unmatched,
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

/// The role a path plays in the ordered default briefing.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadingPurpose {
    StartHere,
    Architecture,
    Runtime,
    Tests,
    SupportingContext,
}

impl ReadingPurpose {
    pub const fn label(self) -> &'static str {
        match self {
            Self::StartHere => "start_here",
            Self::Architecture => "architecture",
            Self::Runtime => "runtime",
            Self::Tests => "tests",
            Self::SupportingContext => "supporting_context",
        }
    }

    pub const fn order(self) -> u8 {
        match self {
            Self::StartHere => 0,
            Self::Architecture => 1,
            Self::Runtime => 2,
            Self::Tests => 3,
            Self::SupportingContext => 4,
        }
    }
}

/// Typed evidence supporting one reading-plan recommendation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadingEvidenceKind {
    Landmark,
    ProjectTopology,
    SourceMap,
    Graph,
    Focus,
    HistoryOverlap,
}

impl ReadingEvidenceKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Landmark => "landmark",
            Self::ProjectTopology => "project_topology",
            Self::SourceMap => "source_map",
            Self::Graph => "graph",
            Self::Focus => "focus",
            Self::HistoryOverlap => "history_overlap",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReadingRecommendation {
    pub ordinal: usize,
    pub purpose: ReadingPurpose,
    pub path: String,
    pub project_root: Option<String>,
    pub reason: String,
    pub evidence_kinds: Vec<ReadingEvidenceKind>,
    pub confidence: ConfidenceTier,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub limitations: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReadingPlanRootOmission {
    pub project_root: String,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReadingPlanShortfall {
    pub target_minimum: usize,
    pub returned: usize,
    pub reason: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReadingPlan {
    pub recommendations: Vec<ReadingRecommendation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub omitted_project_roots: Vec<ReadingPlanRootOmission>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shortfall: Option<ReadingPlanShortfall>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub limitations: Vec<String>,
}

/// Pre-projection evidence retained only while constructing the default plan.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReadingPlanEvidence {
    pub sources: Vec<ReadingSourceEvidence>,
    pub ranking: Vec<FileRank>,
    pub graph: Vec<ReadingGraphEvidence>,
    pub omissions: Vec<SourceOmission>,
    pub landmarks: Vec<Landmark>,
    pub project_roots: Vec<ProjectRoot>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadingSourceEvidence {
    pub path: String,
    pub limitations: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadingGraphEvidence {
    pub source: String,
    pub target: String,
}

/// Typed history-analysis inputs that are also reported for reproducibility.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HistorySettings {
    pub window_days: u32,
    pub recent_window_days: u32,
    pub bug_keywords: Vec<String>,
    pub firefighting_keywords: Vec<String>,
    #[serde(default)]
    pub keyword_match: KeywordMatchMode,
    #[serde(default)]
    pub include_emails: bool,
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EffectiveMapOptions {
    pub excludes: Vec<String>,
    pub focuses: Vec<String>,
    pub focus_paths: Vec<String>,
    pub map_tokens: usize,
    pub cache_mode: CacheMode,
    pub cache_files: Vec<String>,
    #[serde(default)]
    pub recursive: bool,
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
            recursive: false,
        }
    }
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

impl HeadSnapshot {
    pub fn object_format(&self) -> &'static str {
        match self.oid.as_deref().map(str::len) {
            Some(64) => "sha256",
            Some(40) => "sha1",
            _ => "unknown",
        }
    }
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
            keyword_match: KeywordMatchMode::Word,
            include_emails: false,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reading_plan: Option<ReadingPlan>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history: Option<HistoryReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map: Option<MapReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explain: Option<ExplainReport>,
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
                let summary = analysis::briefing_summary(&history_report, &map_report);
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
            CommandName::Explain => {
                let path = req.command.path.clone();
                let target = req.command.target.clone().unwrap_or_default();
                let history_report =
                    history::analyze(&path, req.history.clone(), None, req.profile).map_err(ReportError::History)?;
                let mut map_settings = req.map.clone();
                map_settings.profile = req.profile;
                let map_report = map::analyze(&path, &map_settings).map_err(ReportError::Map)?;
                let explain = analysis::explain_report(&target, &map_report, &history_report);
                let summary = format!(
                    "Explained `{target}` using {} source files and {} retained graph edges within scoped history evidence.",
                    map_report.inventory.analyzed,
                    map_report.edges.len(),
                );
                let mut report = Self::from_parts(req, captured_at, summary, Some(history_report), Some(map_report));
                report.explain = Some(explain);
                Ok(report)
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
                recursive: req.map.recursive,
            },
            history: req.history.clone(),
        };
        let quality = analysis::report_quality(history.as_ref(), map.as_ref());
        let reading_plan = match (req.command.name, history.as_ref(), map.as_ref()) {
            (CommandName::Briefing, Some(history), Some(map)) => Some(analysis::build_reading_plan(history, map)),
            _ => None,
        };
        let provenance = ReportProvenance {
            tool_version: TOOL_VERSION.to_owned(),
            captured_at: captured_at.clone(),
            reference_time: captured_at,
            effective_options,
            repository: RepositoryIdentity {
                canonical_root: repository_root.clone(),
                stable_id: stable_repository_id(&repository_root),
                object_format: head.object_format().to_owned(),
            },
            head,
            worktree,
            languages: analysis::language_provenance(map.as_ref()),
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
            reading_plan,
            history,
            map,
            explain: None,
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

        if self.command.name == CommandName::Briefing {
            if let Some(map) = &self.map {
                Render::briefing_overview(&mut output, map);
            }
            if let Some(reading_plan) = &self.reading_plan {
                Render::reading_plan_markdown(&mut output, reading_plan);
            }
            if let Some(history) = &self.history {
                if self.profile == AnalysisProfile::Compact {
                    Render::history_briefing_markdown(&mut output, history);
                } else {
                    Render::history_markdown(&mut output, history);
                }
            }
            if let Some(map) = &self.map {
                Render::map_markdown(&mut output, map);
            }
        } else {
            if let Some(history) = &self.history {
                Render::history_markdown(&mut output, history);
            }
            if let Some(map) = &self.map {
                Render::map_markdown(&mut output, map);
            }
        }

        if let Some(explain) = &self.explain {
            Render::explain_markdown(&mut output, explain);
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
