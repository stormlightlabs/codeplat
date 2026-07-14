use std::ffi::OsString;
use std::io;
use std::io::{IsTerminal, Write};
use std::path::PathBuf;

use anyhow::Context;
use clap::{ArgAction, ColorChoice, Parser, Subcommand, ValueEnum, builder::Styles, error::ErrorKind};
use owo_colors::OwoColorize;
use thiserror::Error;

use crate::report::{CommandDescriptor, HistoryOperation, Report};

// FIXME: what is this for?
const HELP_AFTER: &str = "Examples:\n  setaryb\n  setaryb map --json\n  setaryb history contributors .\n\nExit status:\n  0  success\n  2  command-line usage error\n  3  repository discovery failure\n  4  input or access failure\n  5  analysis failure\n 70  internal failure";

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
    #[expect(
        dead_code,
        reason = "Ticket 2 will use this stable category for repository-discovery failures."
    )]
    Repository,
    #[expect(
        dead_code,
        reason = "Tickets 2 and 3 will use this stable category for input and access failures."
    )]
    Input,
    #[expect(
        dead_code,
        reason = "Tickets 2 and 3 will use this stable category for analysis failures."
    )]
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

#[derive(Debug, Error)]
enum ApplicationError {
    #[error("{0}")]
    Usage(String),
    #[error("could not serialize the report as JSON")]
    Render(#[source] serde_json::Error),
}

impl ApplicationError {
    fn usage(message: impl Into<String>) -> Self {
        Self::Usage(message.into())
    }

    const fn category(&self) -> ExitCategory {
        match self {
            Self::Usage(_) => ExitCategory::Usage,
            Self::Render(_) => ExitCategory::Internal,
        }
    }
}

#[derive(Debug, Parser)]
#[command(
    name = "setaryb",
    version,
    about = "Read-only repository orientation for people and coding agents.",
    long_about = "Setaryb produces a concise, evidence-backed repository briefing. Use `map` or `history` for focused reports.",
    after_help = HELP_AFTER,
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

impl Cli {
    fn into_request(self) -> CommandRequest {
        let command = match self.command {
            None => CommandDescriptor::briefing(self.path),
            Some(SubcommandName::Map(map)) => CommandDescriptor::map(map.path),
            Some(SubcommandName::History(history)) => match history.operation {
                Some(operation) => {
                    let (operation, path) = operation.into_parts();
                    CommandDescriptor::history(path, Some(operation))
                }
                None => CommandDescriptor::history(history.path, None),
            },
        };

        CommandRequest { command }
    }

    fn output_format(&self) -> Result<OutputFormat, ApplicationError> {
        self.output.format()
    }

    fn color_policy(&self) -> ColorPolicy {
        self.output.color_policy()
    }
}

#[derive(Debug, clap::Args)]
#[command(after_help = HELP_AFTER)]
struct MapCommand {
    #[arg(value_name = "PATH", default_value = ".")]
    path: PathBuf,
}

#[derive(Debug, clap::Args)]
#[command(after_help = HELP_AFTER)]
struct HistoryCommand {
    #[command(subcommand)]
    operation: Option<HistorySubcommand>,

    #[arg(value_name = "PATH", default_value = ".")]
    path: PathBuf,
}

#[derive(Debug, clap::Args)]
#[command(after_help = HELP_AFTER)]
struct HistoryOperationCommand {
    #[arg(value_name = "PATH", default_value = ".")]
    path: PathBuf,
}

impl HistorySubcommand {
    fn into_parts(self) -> (HistoryOperation, PathBuf) {
        match self {
            Self::Churn(command) => (HistoryOperation::Churn, command.path),
            Self::Contributors(command) => (HistoryOperation::Contributors, command.path),
            Self::Bugs(command) => (HistoryOperation::Bugs, command.path),
            Self::Activity(command) => (HistoryOperation::Activity, command.path),
            Self::Firefighting(command) => (HistoryOperation::Firefighting, command.path),
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
                .map_or(ExitCategory::Internal, ApplicationError::category);
            write_diagnostic(stderr, &error, color_policy, stderr_is_terminal, color_environment);
            category
        }
    }
}

fn invoke<W: Write>(cli: Cli, stdout: &mut W) -> anyhow::Result<()> {
    let output_format = cli.output_format()?;
    let report = Report::foundation(cli.into_request());
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
        assert_eq!(error.category(), ExitCategory::Usage);
    }

    #[test]
    fn clap_command_contains_documented_exit_categories() {
        let help = Cli::command().render_long_help().to_string();
        assert!(help.contains("Exit status:"));
        assert!(help.contains("repository discovery failure"));
    }
}
