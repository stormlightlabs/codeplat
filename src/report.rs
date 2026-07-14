use std::fmt::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{
    cli::{CommandRequest, OutputFormat},
    utils,
};

/// The current compatibility version of the JSON report envelope.
pub const SCHEMA_VERSION: u16 = 1;

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
        };

        let markdown = report.render(OutputFormat::Markdown).expect("markdown renders");
        assert!(!markdown.contains('\u{1b}'));
        assert!(!markdown.contains('\u{7}'));
        assert!(markdown.contains("title\\*"));
    }
}
