use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::cli::{CommandRequest, OutputFormat};
use crate::{history, map, utils};

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
}

impl SourceLanguage {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::JavaScript => "javascript",
            Self::JavaScriptJsx => "javascript_jsx",
            Self::TypeScript => "typescript",
            Self::TypeScriptTsx => "typescript_tsx",
        }
    }

    pub const fn display_label(self) -> &'static str {
        match self {
            Self::Rust => "Rust",
            Self::JavaScript => "JavaScript",
            Self::JavaScriptJsx => "JavaScript (JSX)",
            Self::TypeScript => "TypeScript",
            Self::TypeScriptTsx => "TypeScript (TSX)",
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
    Symlink,
    ReadError,
    TraversalError,
}

impl OmissionReason {
    pub const fn label(self) -> &'static str {
        match self {
            Self::IgnoredUntracked => "ignored_untracked",
            Self::UnsupportedLanguage => "unsupported_language",
            Self::ExplicitExclusion => "explicit_exclusion",
            Self::Symlink => "symlink",
            Self::ReadError => "read_error",
            Self::TraversalError => "traversal_error",
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
    pub command: CommandDescriptor,
    pub scope: ReportScope,
    pub status: ReportStatus,
    pub summary: String,
    pub findings: Vec<Finding>,
    pub limitations: Vec<Limitation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history: Option<HistoryReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map: Option<MapReport>,
}

impl Report {
    pub fn foundation(request: CommandRequest) -> Self {
        let (summary, limitations) = match (&request.command.name, request.command.operation) {
            (CommandName::Briefing, _) => (
                "The command and rendering foundation is ready; repository analysis will be added in subsequent tickets.",
                vec![
                    Limitation::new("History analysis is not available in this build."),
                    Limitation::new("Source-map analysis is not available in this build."),
                ],
            ),
            (CommandName::Map, _) => (
                "The map command contract and renderers are ready; source-map analysis will be added in a subsequent ticket.",
                vec![Limitation::new("Source-map analysis is not available in this build.")],
            ),
            (CommandName::History, _) => (
                "The history command contract and renderers are ready; Git-history analysis will be added in a subsequent ticket.",
                vec![Limitation::new("History analysis is not available in this build.")],
            ),
        };

        Self {
            schema_version: SCHEMA_VERSION,
            scope: ReportScope::from(request.command.path.clone()),
            command: request.command,
            status: ReportStatus::Foundation,
            summary: summary.to_owned(),
            findings: Vec::new(),
            limitations,
            history: None,
            map: None,
        }
    }

    pub fn analyze(request: CommandRequest) -> Result<Self, ReportError> {
        match request.command.name {
            CommandName::Briefing => Ok(Self::foundation(request)),
            CommandName::History => {
                let scope = ReportScope::from(request.command.path.clone());
                let history_report =
                    history::analyze(&request.command.path, request.history, request.command.operation)
                        .map_err(ReportError::History)?;

                Ok(Self {
                    schema_version: SCHEMA_VERSION,
                    scope,
                    command: request.command,
                    status: ReportStatus::Analyzed,
                    summary: format!(
                        "Analyzed {} reachable commits, including {} non-merge commits, within the selected history scope.",
                        history_report.commits_seen, history_report.non_merge_commits_seen
                    ),
                    findings: Vec::new(),
                    limitations: Vec::new(),
                    history: Some(history_report),
                    map: None,
                })
            }
            CommandName::Map => {
                let scope = ReportScope::from(request.command.path.clone());
                let map_report = map::analyze(&request.command.path, &request.map).map_err(ReportError::Map)?;
                Ok(Self {
                    schema_version: SCHEMA_VERSION,
                    scope,
                    command: request.command,
                    status: ReportStatus::Analyzed,
                    summary: map_summary(&map_report),
                    findings: Vec::new(),
                    limitations: Vec::new(),
                    history: None,
                    map: Some(map_report),
                })
            }
        }
    }

    /// Render a report from the shared typed model without parsing or transforming Markdown.
    pub fn render(&self, format: OutputFormat) -> Result<String, serde_json::Error> {
        match format {
            OutputFormat::Markdown => Ok(self.render_markdown()),
            OutputFormat::Json => {
                let mut output = serde_json::to_string_pretty(self)?;
                output.push('\n');
                Ok(output)
            }
        }
    }

    fn render_markdown(&self) -> String {
        let mut output = String::new();
        let command = match self.command.operation {
            Some(operation) => format!("{}: {}", self.command.name.label(), operation.label()),
            None => self.command.name.label().to_owned(),
        };

        writeln!(output, "# Setaryb {command}").expect("writing to a string cannot fail");
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

fn map_summary(map: &MapReport) -> String {
    let mut languages = map
        .files
        .iter()
        .map(|file| file.language.display_label())
        .collect::<Vec<_>>();
    languages.sort_unstable();
    languages.dedup();
    if languages.is_empty() || (languages.len() == 1 && languages[0] == SourceLanguage::Rust.display_label()) {
        format!(
            "Analyzed {} Rust source files and recorded {} omitted paths within the selected source scope.",
            map.inventory.analyzed, map.inventory.omitted
        )
    } else {
        format!(
            "Analyzed {} source files ({}) and recorded {} omitted paths within the selected source scope.",
            map.inventory.analyzed,
            languages.join(", "),
            map.inventory.omitted
        )
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HistoryReport {
    pub repository_root: String,
    pub scope_path: String,
    pub settings: HistorySettings,
    pub commits_seen: usize,
    pub non_merge_commits_seen: usize,
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MapReport {
    pub repository_root: String,
    pub scope_path: String,
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

impl Limitation {
    fn new(detail: impl Into<String>) -> Self {
        Self { detail: detail.into() }
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
        if !map.exclusions.is_empty() {
            writeln!(
                output,
                "Exclusions: `{}`",
                utils::escape_inline_code(&map.exclusions.join("`, `"))
            )
            .expect("writing to a string cannot fail");
        }

        if !map.files.is_empty() {
            writeln!(
                output,
                "Cache: {} ({}) — {} hits, {} misses, {} refreshed, {} stale",
                map.cache.mode.label(),
                map.cache.status.label(),
                map.cache.hits,
                map.cache.misses,
                map.cache.refreshed.len(),
                map.cache.stale.len()
            )
            .expect("writing to a string cannot fail");
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
            command: CommandDescriptor::map(PathBuf::from("unsafe\u{1b}[31m-path")),
            scope: ReportScope { selected_path: "unsafe\u{1b}[31m-path".to_owned() },
            status: ReportStatus::Foundation,
            summary: "A\u{1b}[31m summary".to_owned(),
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
}
