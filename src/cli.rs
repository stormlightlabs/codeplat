use std::ffi::OsString;
use std::io;
use std::io::{IsTerminal, Write};
use std::path::PathBuf;

use anyhow::Context;
use clap::{
    ArgAction, ColorChoice, Command, CommandFactory, Parser, Subcommand, ValueEnum, builder::Styles, error::ErrorKind,
};
use owo_colors::OwoColorize;
use serde::{Deserialize, Serialize};

use crate::map::{CacheCommand, CacheControlReport, MapSettings};
use crate::report::{
    AnalysisProfile, CacheMode, CapabilitiesReport, CommandDescriptor, DoctorReport, HistoryOperation, HistorySettings,
    KeywordMatchMode, Report, StrictIssue,
};
use crate::utils;

#[derive(Debug, Subcommand)]
enum SubcommandName {
    /// Produce only the structural repository map.
    Map(MapCommand),
    /// Produce Git-history findings, or select one focused history signal.
    History(HistoryCommand),
    /// Inspect or control retained source-analysis cache data.
    Cache(CacheCommandCli),
    /// Report installed schema, language, query-pack, and limit capabilities.
    Capabilities,
    /// Explain the bounded evidence behind a path or symbol recommendation.
    Explain(ExplainCommand),
    /// Check local discovery and Codeplat support without analyzing source.
    Doctor(DoctorCommand),
}

#[derive(Clone, Copy, Debug, clap::Subcommand)]
enum CacheOperation {
    /// Print the configured cache root.
    Path,
    /// Report cache record count, size, repositories, and retention limits.
    Status,
    /// Remove expired and over-limit records.
    Prune,
    /// Remove all Codeplat cache records.
    Clear,
}

impl From<CacheOperation> for CacheCommand {
    fn from(operation: CacheOperation) -> Self {
        match operation {
            CacheOperation::Path => Self::Path,
            CacheOperation::Status => Self::Status,
            CacheOperation::Prune => Self::Prune,
            CacheOperation::Clear => Self::Clear,
        }
    }
}

#[derive(Debug, Subcommand)]
enum HistorySubcommand {
    /// Show changed-path frequency over the configured time window.
    Churn(HistoryOperationCommand),
    /// Show commit-author concentration.
    Contributors(HistoryOperationCommand),
    /// Show fix-related path clusters and churn overlap.
    Bugs(HistoryOperationCommand),
    /// Show author-date activity grouped by month.
    Activity(HistoryOperationCommand),
    /// Show commits using firefighting language.
    Firefighting(HistoryOperationCommand),
}

/// The report serialization selected by `--format` or `--json`.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    #[default]
    Markdown,
    Json,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum CacheModeOption {
    Auto,
    Always,
    Files,
    Manual,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum ProfileOption {
    Compact,
    Evidence,
}

impl From<ProfileOption> for AnalysisProfile {
    fn from(profile: ProfileOption) -> Self {
        match profile {
            ProfileOption::Compact => Self::Compact,
            ProfileOption::Evidence => Self::Evidence,
        }
    }
}

#[derive(Debug, clap::Args)]
struct DoctorCommand {
    #[arg(
        value_name = "PATH",
        default_value = ".",
        help = "Repository or subdirectory to inspect (default: current directory)."
    )]
    path: PathBuf,
}

impl From<CacheModeOption> for CacheMode {
    fn from(mode: CacheModeOption) -> Self {
        match mode {
            CacheModeOption::Auto => Self::Auto,
            CacheModeOption::Always => Self::Always,
            CacheModeOption::Files => Self::Files,
            CacheModeOption::Manual => Self::Manual,
        }
    }
}

/// The diagnostic color policy selected by `--color` or `--no-color`.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum ColorPolicy {
    #[default]
    Auto,
    Always,
    Never,
}

impl ColorPolicy {
    fn should_color(self, is_terminal: bool, environment: ColorEnvironment) -> bool {
        match self {
            Self::Always => true,
            Self::Never => false,
            Self::Auto => is_terminal && !environment.no_color && !environment.term_is_dumb,
        }
    }
}

/// Stable categories for command termination.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExitCategory {
    Success,
    Usage,
    Repository,
    Input,
    Analysis,
    Internal,
}

impl ExitCategory {
    /// The process status documented in command help.
    pub const fn code(self) -> i32 {
        match self {
            Self::Success => 0,
            Self::Usage => 2,
            Self::Repository => 3,
            Self::Input => 4,
            Self::Analysis => 5,
            Self::Internal => 70,
        }
    }
}

#[derive(Debug, thiserror::Error)]
enum ApplicationError {
    #[error("{0}")]
    Usage(String),
    #[error("{0}")]
    Report(#[source] crate::report::ReportError),
    #[error("could not serialize the report as JSON")]
    Render(#[source] serde_json::Error),
    #[error("strict report policy rejected: {issues:?}")]
    Strict { issues: Vec<StrictIssue> },
    #[error("doctor found one or more failing checks")]
    DoctorFailed,
}

impl From<ApplicationError> for ExitCategory {
    fn from(error: ApplicationError) -> Self {
        match error {
            ApplicationError::Usage(_) => ExitCategory::Usage,
            ApplicationError::Report(error) => match error {
                crate::report::ReportError::History(error) => error.into(),
                crate::report::ReportError::Map(error) => error.into(),
            },
            ApplicationError::Render(_) => ExitCategory::Internal,
            ApplicationError::Strict { .. } => ExitCategory::Analysis,
            ApplicationError::DoctorFailed => ExitCategory::Repository,
        }
    }
}

impl From<&ApplicationError> for ExitCategory {
    fn from(value: &ApplicationError) -> Self {
        match value {
            ApplicationError::Usage(_) => ExitCategory::Usage,
            ApplicationError::Report(error) => match error {
                crate::report::ReportError::History(error) => error.into(),
                crate::report::ReportError::Map(error) => error.into(),
            },
            ApplicationError::Render(_) => ExitCategory::Internal,
            ApplicationError::Strict { .. } => ExitCategory::Analysis,
            ApplicationError::DoctorFailed => ExitCategory::Repository,
        }
    }
}

impl ApplicationError {
    fn usage(message: impl Into<String>) -> Self {
        Self::Usage(message.into())
    }
}

#[derive(Debug, clap::Args)]
struct CacheCommandCli {
    #[command(subcommand)]
    operation: CacheOperation,
}

#[derive(Debug, Parser)]
#[command(
    name = "codeplat",
    version,
    about = "Read-only repository orientation for people and coding agents.",
    long_about = "Codeplat produces a concise, evidence-backed repository briefing.

The default command combines Git-history signals with a ranked source map.

Use `map` or `history` for focused reports.

Examples:
    codeplat .
    codeplat --json .
    codeplat --focus parser --focus-path src .
    codeplat --no-cache .
    codeplat map --json
    codeplat history contributors .
    codeplat explain src/parser.rs --json
    codeplat capabilities --json
    codeplat doctor --json

See https://github.com/stormlightlabs/codeplat/issues for support and bug reports.

Exit status:
    0  success
    2  command-line usage error
    3  repository discovery failure
    4  input or access failure
    5  analysis failure
    70  internal failure
",
    color = ColorChoice::Never,
    styles = Styles::plain(),
    disable_help_subcommand = true
)]
struct Cli {
    #[command(flatten)]
    output: OutputOptions,

    #[command(flatten)]
    map_options: MapOptions,

    #[command(subcommand)]
    command: Option<SubcommandName>,

    #[arg(
        value_name = "PATH",
        default_value = ".",
        help = "Repository or subdirectory to analyze (default: current directory)."
    )]
    path: PathBuf,
}

/// Build the authoritative Clap command used by help, completions, and man pages.
pub fn command() -> Command {
    Cli::command()
}

impl From<Cli> for CommandRequest {
    fn from(cli: Cli) -> Self {
        let output_format =
            cli.output
                .format
                .unwrap_or(if cli.output.json { OutputFormat::Json } else { OutputFormat::Markdown });
        let color_policy = cli.color_policy();
        let strict = cli.output.strict;
        let profile = cli.output.profile.into();
        let default_map_settings = cli.map_options.settings();
        let (command, history, map_settings) = match cli.command {
            None => (
                CommandDescriptor::briefing(cli.path),
                HistorySettings::default(),
                default_map_settings,
            ),
            Some(SubcommandName::Map(map)) => {
                let MapCommand { options, path } = map;
                (
                    CommandDescriptor::map(path),
                    HistorySettings::default(),
                    options.settings(),
                )
            }
            Some(SubcommandName::History(history)) => {
                let inherited = history.options.settings();
                match history.operation {
                    Some(operation) => {
                        let (operation, path, settings) = operation.into_parts(&inherited);
                        (
                            CommandDescriptor::history(path, Some(operation)),
                            settings,
                            MapSettings::default(),
                        )
                    }
                    None => (
                        CommandDescriptor::history(history.path, None),
                        inherited,
                        MapSettings::default(),
                    ),
                }
            }
            Some(SubcommandName::Cache(_)) | Some(SubcommandName::Capabilities) | Some(SubcommandName::Doctor(_)) => (
                CommandDescriptor::map(cli.path),
                HistorySettings::default(),
                default_map_settings,
            ),
            Some(SubcommandName::Explain(explain)) => (
                CommandDescriptor::explain(explain.target, explain.path),
                HistorySettings::default(),
                explain.options.settings(),
            ),
        };

        let mut map = map_settings;
        map.profile = profile;
        CommandRequest { command, history, map, profile, output_format, color_policy, strict }
    }
}

impl Cli {
    fn output_format(&self) -> Result<OutputFormat, ApplicationError> {
        self.output.format()
    }

    fn color_policy(&self) -> ColorPolicy {
        self.output.color_policy()
    }

    fn validate(&self) -> Result<(), ApplicationError> {
        self.map_options.validate()?;
        if let Some(SubcommandName::Map(map)) = &self.command {
            map.options.validate()?;
        }
        if let Some(SubcommandName::Explain(explain)) = &self.command {
            explain.options.validate()?;
            if explain.target.trim().is_empty() {
                return Err(ApplicationError::usage(
                    "`explain` requires a non-empty path or symbol target",
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, clap::Args)]
#[command(after_help = "Examples:
    codeplat map
    codeplat map --json

Support: https://github.com/stormlightlabs/codeplat/issues
")]
struct MapCommand {
    #[command(flatten)]
    options: MapOptions,

    #[arg(
        value_name = "PATH",
        default_value = ".",
        help = "Repository or subdirectory to analyze (default: current directory)."
    )]
    path: PathBuf,
}

#[derive(Debug, clap::Args)]
#[command(after_help = "Examples:
    codeplat explain src/parser.rs --json
    codeplat explain Parser --focus Parser --json

The explanation reports bounded focus, history, landmark, graph, ranking,
ambiguity, and omission evidence. It is heuristic evidence, not a semantic call graph.

Support: https://github.com/stormlightlabs/codeplat/issues
")]
struct ExplainCommand {
    #[command(flatten)]
    options: MapOptions,

    #[arg(value_name = "PATH-OR-SYMBOL", help = "Path or symbol to explain.")]
    target: String,

    #[arg(
        value_name = "PATH",
        default_value = ".",
        help = "Repository or subdirectory to analyze."
    )]
    path: PathBuf,
}

#[derive(Clone, Debug, clap::Args)]
struct MapOptions {
    /// Exclude paths from analysis using a Git-style glob; repeat for multiple exclusions.
    #[arg(long = "exclude", value_name = "GLOB", action = ArgAction::Append)]
    excludes: Vec<String>,

    /// Boost symbols and paths matching this explicit task text; repeat for multiple terms.
    #[arg(long = "focus", value_name = "TEXT", action = ArgAction::Append)]
    focuses: Vec<String>,

    /// Boost files under this explicit path; repeat for multiple paths.
    #[arg(long = "focus-path", value_name = "PATH", action = ArgAction::Append)]
    focus_paths: Vec<String>,

    /// Maximum estimated tokens in the selected structural map (default: 1000).
    #[arg(long = "map-tokens", value_name = "N", default_value_t = 1_000, value_parser = clap::value_parser!(usize))]
    map_tokens: usize,

    /// Cache policy: auto, always, files, or manual (default: auto).
    #[arg(
        long = "cache",
        visible_alias = "cache-mode",
        value_name = "MODE",
        value_enum,
        default_value_t = CacheModeOption::Auto
    )]
    cache_mode: CacheModeOption,

    /// Refresh only these paths when `--cache files` is selected; repeat as needed.
    #[arg(long = "cache-file", visible_alias = "changed-file", value_name = "PATH", action = ArgAction::Append)]
    cache_files: Vec<String>,

    /// Disable all cache reads and writes.
    #[arg(long = "no-cache", action = ArgAction::SetTrue)]
    no_cache: bool,

    /// Descend into nested repositories and checked-out submodules.
    #[arg(long = "recursive", action = ArgAction::SetTrue)]
    recursive: bool,
}

impl From<MapOptions> for MapSettings {
    fn from(options: MapOptions) -> Self {
        options.settings()
    }
}

impl MapOptions {
    fn settings(&self) -> MapSettings {
        MapSettings {
            excludes: self.excludes.clone(),
            focuses: self.focuses.clone(),
            focus_paths: self.focus_paths.clone(),
            map_tokens: self.map_tokens,
            cache_mode: if self.no_cache { CacheMode::Disabled } else { self.cache_mode.into() },
            cache_files: self.cache_files.clone(),
            recursive: self.recursive,
            profile: AnalysisProfile::Compact,
        }
    }

    fn validate(&self) -> Result<(), ApplicationError> {
        if self.map_tokens == 0 {
            return Err(ApplicationError::usage("`--map-tokens` must be greater than zero"));
        }
        if self.no_cache && self.cache_mode != CacheModeOption::Auto {
            return Err(ApplicationError::usage(
                "`--no-cache` cannot be combined with an explicit `--cache` mode",
            ));
        }
        if self.cache_mode == CacheModeOption::Files && self.cache_files.is_empty() && !self.no_cache {
            return Err(ApplicationError::usage(
                "`--cache files` requires at least one `--cache-file` path",
            ));
        }
        if self.cache_files.iter().any(|path| path.trim().is_empty()) {
            return Err(ApplicationError::usage("`--cache-file` paths must not be empty"));
        }
        Ok(())
    }
}

#[derive(Debug, clap::Args)]
#[command(after_help = "Examples:
    codeplat history
    codeplat history contributors .

Support: https://github.com/stormlightlabs/codeplat/issues
")]
struct HistoryCommand {
    #[command(flatten)]
    options: HistoryOptions,

    #[command(subcommand)]
    operation: Option<HistorySubcommand>,

    #[arg(
        value_name = "PATH",
        default_value = ".",
        help = "Repository or subdirectory to analyze (default: current directory)."
    )]
    path: PathBuf,
}

#[derive(Debug, clap::Args)]
#[command(after_help = "Examples:
    codeplat history churn
    codeplat history bugs --json

Support: https://github.com/stormlightlabs/codeplat/issues
")]
struct HistoryOperationCommand {
    #[command(flatten)]
    options: HistoryOptions,

    #[arg(
        value_name = "PATH",
        default_value = ".",
        help = "Repository or subdirectory to analyze (default: current directory)."
    )]
    path: PathBuf,
}

impl HistorySubcommand {
    fn into_parts(self, inherited: &HistorySettings) -> (HistoryOperation, PathBuf, HistorySettings) {
        match self {
            Self::Churn(command) => (
                HistoryOperation::Churn,
                command.path,
                command.options.settings_with_fallback(inherited),
            ),
            Self::Contributors(command) => (
                HistoryOperation::Contributors,
                command.path,
                command.options.settings_with_fallback(inherited),
            ),
            Self::Bugs(command) => (
                HistoryOperation::Bugs,
                command.path,
                command.options.settings_with_fallback(inherited),
            ),
            Self::Activity(command) => (
                HistoryOperation::Activity,
                command.path,
                command.options.settings_with_fallback(inherited),
            ),
            Self::Firefighting(command) => (
                HistoryOperation::Firefighting,
                command.path,
                command.options.settings_with_fallback(inherited),
            ),
        }
    }
}

#[derive(Clone, Debug, Default, clap::Args)]
struct HistoryOptions {
    /// Number of trailing days for churn, bug, and firefighting signals (default: 365).
    #[arg(long, value_name = "DAYS", value_parser = clap::value_parser!(u32).range(1..))]
    window_days: Option<u32>,

    /// Number of trailing days used for recent contributor concentration (default: 180).
    #[arg(long, value_name = "DAYS", value_parser = clap::value_parser!(u32).range(1..))]
    recent_window_days: Option<u32>,

    /// Replace the default bug-message keywords; repeat for multiple words (default: fix, bug, broken).
    #[arg(long = "bug-keyword", value_name = "WORD", action = ArgAction::Append)]
    bug_keywords: Vec<String>,

    /// Replace the default firefighting keywords; repeat for multiple words (default: revert, hotfix, emergency, rollback).
    #[arg(long = "firefighting-keyword", value_name = "WORD", action = ArgAction::Append)]
    firefighting_keywords: Vec<String>,

    /// Keyword matching policy: word (default) or substring compatibility mode.
    #[arg(long = "keyword-match", value_name = "MODE", value_enum)]
    keyword_match: Option<KeywordMatchModeOption>,

    /// Include contributor email addresses in reports and mailmap provenance.
    #[arg(long, action = ArgAction::SetTrue)]
    include_emails: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum KeywordMatchModeOption {
    Word,
    Substring,
}

impl From<KeywordMatchModeOption> for KeywordMatchMode {
    fn from(mode: KeywordMatchModeOption) -> Self {
        match mode {
            KeywordMatchModeOption::Word => Self::Word,
            KeywordMatchModeOption::Substring => Self::Substring,
        }
    }
}

impl HistoryOptions {
    fn settings(&self) -> HistorySettings {
        self.settings_with_fallback(&HistorySettings::default())
    }

    fn settings_with_fallback(&self, fallback: &HistorySettings) -> HistorySettings {
        HistorySettings {
            window_days: self.window_days.unwrap_or(fallback.window_days),
            recent_window_days: self.recent_window_days.unwrap_or(fallback.recent_window_days),
            bug_keywords: if self.bug_keywords.is_empty() {
                fallback.bug_keywords.clone()
            } else {
                self.bug_keywords.clone()
            },
            firefighting_keywords: if self.firefighting_keywords.is_empty() {
                fallback.firefighting_keywords.clone()
            } else {
                self.firefighting_keywords.clone()
            },
            keyword_match: self.keyword_match.map(Into::into).unwrap_or(fallback.keyword_match),
            include_emails: self.include_emails || fallback.include_emails,
        }
    }
}

#[derive(Debug, clap::Args)]
struct OutputOptions {
    /// Select Markdown for people or JSON for tools.
    #[arg(long, global = true, value_enum)]
    format: Option<OutputFormat>,

    /// Shorthand for `--format json`.
    #[arg(long, global = true, action = ArgAction::SetTrue)]
    json: bool,

    /// Control diagnostic color; report stdout is always uncolored.
    #[arg(long, global = true, value_enum, default_value_t = ColorPolicy::Auto)]
    color: ColorPolicy,

    /// Alias for `--color never`.
    #[arg(long = "no-color", global = true, action = ArgAction::SetTrue)]
    no_color: bool,

    /// Evidence profile: compact (default) or bounded exhaustive evidence.
    #[arg(long, global = true, value_enum, default_value_t = ProfileOption::Compact)]
    profile: ProfileOption,

    /// Fail after rendering when actionable evidence is stale, resource-limited, unsafe, unsupported, or partial.
    #[arg(long, global = true, action = ArgAction::SetTrue)]
    strict: bool,
}

impl OutputOptions {
    fn format(&self) -> Result<OutputFormat, ApplicationError> {
        match (self.format, self.json) {
            (Some(OutputFormat::Markdown), true) => Err(ApplicationError::usage(
                "`--json` cannot be combined with `--format markdown`; choose one output format",
            )),
            (Some(format), _) => Ok(format),
            (None, true) => Ok(OutputFormat::Json),
            (None, false) => Ok(OutputFormat::Markdown),
        }
    }

    fn color_policy(&self) -> ColorPolicy {
        if self.no_color { ColorPolicy::Never } else { self.color }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ColorEnvironment {
    no_color: bool,
    term_is_dumb: bool,
}

impl ColorEnvironment {
    fn from_process() -> Self {
        Self {
            no_color: std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty()),
            term_is_dumb: matches!(std::env::var("TERM"), Ok(term) if term == "dumb"),
        }
    }
}

#[derive(Debug)]
pub struct CommandRequest {
    pub command: CommandDescriptor,
    pub history: HistorySettings,
    pub map: MapSettings,
    pub profile: AnalysisProfile,
    pub output_format: OutputFormat,
    pub color_policy: ColorPolicy,
    pub strict: bool,
}

/// Parse and execute the command line using the process environment and standard streams.
pub fn run() -> i32 {
    let mut stdout = io::stdout().lock();
    let mut stderr = io::stderr().lock();
    let color_environment = ColorEnvironment::from_process();
    let stderr_is_terminal = stderr.is_terminal();

    run_from_with_environment(
        std::env::args_os(),
        &mut stdout,
        &mut stderr,
        stderr_is_terminal,
        color_environment,
    )
    .code()
}

fn run_from_with_environment<I, T, W, E>(
    arguments: I, stdout: &mut W, stderr: &mut E, stderr_is_terminal: bool, color_environment: ColorEnvironment,
) -> ExitCategory
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
    W: Write,
    E: Write,
{
    let cli = match Cli::try_parse_from(arguments) {
        Ok(cli) => cli,
        Err(error) => {
            return match error.kind() {
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => match write!(stdout, "{}", error.render()) {
                    Ok(()) => ExitCategory::Success,
                    Err(write_error) => {
                        let _ = writeln!(stderr, "error: could not write command help: {write_error}");
                        ExitCategory::Internal
                    }
                },
                _ => {
                    if write!(stderr, "{}", error.render()).is_ok() {
                        ExitCategory::Usage
                    } else {
                        ExitCategory::Internal
                    }
                }
            };
        }
    };

    let color_policy = cli.color_policy();

    match invoke(cli, stdout, stderr, stderr_is_terminal).context("could not invoke Codeplat") {
        Ok(()) => ExitCategory::Success,
        Err(error) => {
            let category = error
                .downcast_ref::<ApplicationError>()
                .map_or(ExitCategory::Internal, |v| v.into());
            write_diagnostic(stderr, &error, color_policy, stderr_is_terminal, color_environment);
            category
        }
    }
}

fn invoke<W: Write, E: Write>(
    cli: Cli, stdout: &mut W, stderr: &mut E, stderr_is_terminal: bool,
) -> anyhow::Result<()> {
    let output_format = cli.output_format()?;
    cli.validate()?;
    if matches!(&cli.command, Some(SubcommandName::Capabilities)) {
        let report = CapabilitiesReport::current();
        let output = report.render(output_format).map_err(ApplicationError::Render)?;
        write_stdout(stdout, output.as_bytes(), "capabilities report")?;
        return Ok(());
    }
    if let Some(SubcommandName::Doctor(command)) = &cli.command {
        let report = DoctorReport::run(command.path.clone());
        let output = report.render(output_format).map_err(ApplicationError::Render)?;
        write_stdout(stdout, output.as_bytes(), "doctor report")?;
        if !report.is_ok() {
            return Err(ApplicationError::DoctorFailed.into());
        }
        return Ok(());
    }
    if let Some(SubcommandName::Cache(cache)) = &cli.command {
        if stderr_is_terminal {
            let _ = writeln!(stderr, "codeplat: reading cache metadata…");
        }
        let report = crate::map::cache_control(cache.operation.into())
            .map_err(|error| ApplicationError::Report(crate::report::ReportError::Map(error)))?;
        let output = render_cache_control(&report, output_format)?;
        write_stdout(stdout, output.as_bytes(), "cache report")?;
        return Ok(());
    }
    if stderr_is_terminal {
        let _ = writeln!(stderr, "codeplat: analyzing repository…");
    }
    let strict = cli.output.strict;
    let report = Report::analyze(cli.into()).map_err(ApplicationError::Report)?;
    let output = report.render(output_format).map_err(ApplicationError::Render)?;
    let strict_issues = report.quality.strict_issues.clone();

    write_stdout(stdout, output.as_bytes(), "report")?;
    if strict && !strict_issues.is_empty() {
        return Err(ApplicationError::Strict { issues: strict_issues }.into());
    }
    Ok(())
}

fn write_stdout<W: Write>(stdout: &mut W, bytes: &[u8], label: &str) -> anyhow::Result<()> {
    match stdout.write_all(bytes) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => return Ok(()),
        Err(error) => return Err(error).context(format!("could not write the {label} to stdout")),
    }
    match stdout.flush() {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        Err(error) => Err(error).context(format!("could not flush {label} stdout")),
    }
}

fn render_cache_control(report: &CacheControlReport, format: OutputFormat) -> Result<String, ApplicationError> {
    match format {
        OutputFormat::Json => {
            let mut output = serde_json::to_string_pretty(report).map_err(ApplicationError::Render)?;
            output.push('\n');
            Ok(output)
        }
        OutputFormat::Markdown => {
            let path = utils::escape_inline_code(report.path.as_deref().unwrap_or("not configured"));
            let mut output = format!("# Codeplat cache {}\n\n", report.operation);
            output.push_str(&format!("Path: `{path}`\n"));
            output.push_str(&format!("Exists: {}\n", report.exists));
            output.push_str(&format!(
                "Records: {} ({} bytes) across {} repositories\n",
                report.records, report.bytes, report.repositories
            ));
            output.push_str(&format!(
                "Retention: {} records, {} bytes, {} seconds\n",
                report.max_records_per_repository, report.max_bytes_per_repository, report.max_age_seconds
            ));
            if report.removed_records > 0 {
                output.push_str(&format!(
                    "Removed: {} records ({} bytes)\n",
                    report.removed_records, report.removed_bytes
                ));
            }
            Ok(output)
        }
    }
}

fn write_diagnostic<W: Write, D: std::fmt::Display>(
    stderr: &mut W, error: D, color_policy: ColorPolicy, stderr_is_terminal: bool, color_environment: ColorEnvironment,
) {
    let label = if color_policy.should_color(stderr_is_terminal, color_environment) {
        "error".red().to_string()
    } else {
        "error".to_owned()
    };
    let _ = writeln!(stderr, "{label}: {error:#}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn no_color_alias_overrides_the_color_flag() {
        let options = OutputOptions {
            format: None,
            json: false,
            color: ColorPolicy::Always,
            no_color: true,
            profile: ProfileOption::Compact,
            strict: false,
        };
        assert_eq!(options.color_policy(), ColorPolicy::Never);
    }

    #[test]
    fn auto_color_respects_terminal_and_environment() {
        let neutral = ColorEnvironment { no_color: false, term_is_dumb: false };
        let no_color = ColorEnvironment { no_color: true, term_is_dumb: false };
        let dumb_terminal = ColorEnvironment { no_color: false, term_is_dumb: true };

        assert!(ColorPolicy::Auto.should_color(true, neutral));
        assert!(!ColorPolicy::Auto.should_color(false, neutral));
        assert!(!ColorPolicy::Auto.should_color(true, no_color));
        assert!(!ColorPolicy::Auto.should_color(true, dumb_terminal));
        assert!(ColorPolicy::Always.should_color(false, no_color));
        assert!(!ColorPolicy::Never.should_color(true, neutral));
    }

    #[test]
    fn contradictory_output_flags_are_usage_errors() {
        let options = OutputOptions {
            format: Some(OutputFormat::Markdown),
            json: true,
            color: ColorPolicy::Auto,
            no_color: false,
            profile: ProfileOption::Compact,
            strict: false,
        };

        let error = options.format().expect_err("options should conflict");
        let cat: ExitCategory = error.into();
        assert_eq!(cat, ExitCategory::Usage);
    }

    #[test]
    fn clap_command_contains_documented_exit_categories() {
        let help = Cli::command().render_long_help().to_string();
        assert!(help.contains("Exit status:"));
        assert!(help.contains("repository discovery failure"));
    }

    struct BrokenPipeWriter;

    impl Write for BrokenPipeWriter {
        fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed pipe"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn broken_pipe_report_output_is_a_quiet_success() {
        let mut writer = BrokenPipeWriter;
        assert!(write_stdout(&mut writer, b"report", "report").is_ok());
    }
}
