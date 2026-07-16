use super::*;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CommandDescriptor {
    pub name: CommandName,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation: Option<HistoryOperation>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub target: Option<String>,
    #[serde(skip)]
    pub path: PathBuf,
}

impl CommandDescriptor {
    pub fn briefing(path: PathBuf) -> Self {
        Self { name: CommandName::Briefing, operation: None, target: None, path }
    }

    pub fn map(path: PathBuf) -> Self {
        Self { name: CommandName::Map, operation: None, target: None, path }
    }

    pub fn history(path: PathBuf, operation: Option<HistoryOperation>) -> Self {
        Self { name: CommandName::History, operation, target: None, path }
    }

    pub fn explain(target: String, path: PathBuf) -> Self {
        Self { name: CommandName::Explain, operation: None, target: Some(target), path }
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DoctorCheck {
    pub name: String,
    pub status: DoctorCheckStatus,
    pub detail: String,
}

impl DoctorCheck {
    fn cache() -> Self {
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
            match serde_json::from_str::<serde_json::Value>(include_str!("../../schema/v1/codeplat.json")) {
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

        checks.push(DoctorCheck::cache());
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
