use std::ffi::OsString;
use std::io;
use std::io::{IsTerminal, Write};
use std::path::PathBuf;

use anyhow::Context;
use clap::{ArgAction, ColorChoice, Parser, Subcommand, ValueEnum, builder::Styles, error::ErrorKind};
use owo_colors::OwoColorize;

use crate::report::{CommandDescriptor, HistoryOperation, HistorySettings, Report};

#[derive(Debug, Subcommand)]
enum SubcommandName {
    /// Produce only the structural repository map.
    Map(MapCommand),
    /// Produce Git-history findings, or select one focused history signal.
    History(HistoryCommand),
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
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum OutputFormat {
    Markdown,
    Json,
}

/// The diagnostic color policy selected by `--color` or `--no-color`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum ColorPolicy {
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
    History(#[source] crate::history::HistoryError),
    #[error("could not serialize the report as JSON")]
    Render(#[source] serde_json::Error),
}

impl Into<ExitCategory> for ApplicationError {
    fn into(self) -> ExitCategory {
        match self {
            Self::Usage(_) => ExitCategory::Usage,
            Self::History(error) => error.into(),
            Self::Render(_) => ExitCategory::Internal,
        }
    }
}

impl From<&ApplicationError> for ExitCategory {
    fn from(value: &ApplicationError) -> Self {
        value.into()
    }
}

impl ApplicationError {
    fn usage(message: impl Into<String>) -> Self {
        Self::Usage(message.into())
    }
}

#[derive(Debug, Parser)]
#[command(
    name = "setaryb",
    version,
    about = "Read-only repository orientation for people and coding agents.",
    long_about = "Setaryb produces a concise, evidence-backed repository briefing.

Use `map` or `history` for focused reports.

Examples:
    setaryb
    setaryb map --json
    setaryb history contributors .

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

    #[command(subcommand)]
    command: Option<SubcommandName>,

    #[arg(value_name = "PATH", default_value = ".")]
    path: PathBuf,
}

impl Into<CommandRequest> for Cli {
    fn into(self) -> CommandRequest {
        let (command, history) = match self.command {
            None => (CommandDescriptor::briefing(self.path), HistorySettings::default()),
            Some(SubcommandName::Map(map)) => (CommandDescriptor::map(map.path), HistorySettings::default()),
            Some(SubcommandName::History(history)) => {
                let inherited = history.options.settings();
                match history.operation {
                    Some(operation) => {
                        let (operation, path, settings) = operation.into_parts(&inherited);
                        (CommandDescriptor::history(path, Some(operation)), settings)
                    }
                    None => (CommandDescriptor::history(history.path, None), inherited),
                }
            }
        };

        CommandRequest { command, history }
    }
}

impl Cli {
    fn output_format(&self) -> Result<OutputFormat, ApplicationError> {
        self.output.format()
    }

    fn color_policy(&self) -> ColorPolicy {
        self.output.color_policy()
    }
}

#[derive(Debug, clap::Args)]
#[command(after_help = "Examples:
    setaryb map
    setaryb map --json
")]
struct MapCommand {
    #[arg(value_name = "PATH", default_value = ".")]
    path: PathBuf,
}

#[derive(Debug, clap::Args)]
#[command(after_help = "Examples:
    setaryb history
    setaryb history contributors .
")]
struct HistoryCommand {
    #[command(flatten)]
    options: HistoryOptions,

    #[command(subcommand)]
    operation: Option<HistorySubcommand>,

    #[arg(value_name = "PATH", default_value = ".")]
    path: PathBuf,
}

#[derive(Debug, clap::Args)]
#[command(after_help = "Examples:
    setaryb history churn
    setaryb history bugs --json
")]
struct HistoryOperationCommand {
    #[command(flatten)]
    options: HistoryOptions,

    #[arg(value_name = "PATH", default_value = ".")]
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

    match invoke(cli, stdout).context("could not invoke Setaryb") {
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

fn invoke<W: Write>(cli: Cli, stdout: &mut W) -> anyhow::Result<()> {
    let output_format = cli.output_format()?;
    let report = Report::analyze(cli.into()).map_err(ApplicationError::History)?;
    let output = report.render(output_format).map_err(ApplicationError::Render)?;

    stdout
        .write_all(output.as_bytes())
        .context("could not write the report to stdout")?;
    stdout.flush().context("could not flush report stdout")?;
    Ok(())
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
        let options = OutputOptions { format: None, json: false, color: ColorPolicy::Always, no_color: true };
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
}
