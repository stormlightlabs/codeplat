use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use ignore::WalkBuilder;
use ignore::gitignore::Gitignore;

use crate::report::{Landmark, LandmarkKind, ProjectRoot, ProjectRootKind, ReportLimits, WorktreeState};
use crate::security;

#[derive(Debug, Default)]
pub struct LandmarkAnalysis {
    pub landmarks: Vec<Landmark>,
    pub project_roots: Vec<ProjectRoot>,
    pub limitations: Vec<String>,
}

pub struct LandmarkAnalysisOptions<'a> {
    pub repository_root: &'a Path,
    pub scope_root: &'a Path,
    pub scope_path: &'a str,
    pub path_states: &'a BTreeMap<String, WorktreeState>,
    pub submodule_paths: &'a BTreeSet<String>,
    pub exclusions: Option<&'a Gitignore>,
    pub limits: &'a ReportLimits,
    pub recursive: bool,
    pub focuses: &'a [String],
    pub focus_paths: &'a [String],
}

pub fn analyze(options: LandmarkAnalysisOptions<'_>) -> LandmarkAnalysis {
    let LandmarkAnalysisOptions {
        repository_root,
        scope_root,
        scope_path,
        path_states,
        submodule_paths,
        exclusions,
        limits,
        recursive,
        focuses,
        focus_paths,
    } = options;
    let mut limitations = Vec::new();
    let mut roots = detect_project_roots(
        repository_root,
        scope_root,
        scope_path,
        path_states,
        exclusions,
        limits,
        &mut limitations,
    );
    let mut collector = LandmarkCollector::new(focuses, focus_paths);

    for root in &roots {
        let kind = match root.kind {
            ProjectRootKind::Workspace => LandmarkKind::WorkspaceRoot,
            ProjectRootKind::Package => LandmarkKind::PackageRoot,
            ProjectRootKind::Mixed | ProjectRootKind::Unknown => LandmarkKind::Unknown,
        };
        let reason = if matches!(root.kind, ProjectRootKind::Mixed | ProjectRootKind::Unknown) {
            format!(
                "project root role is unknown because the manifest families or contents were unavailable or conflicting: {}",
                root.manifests.join(", ")
            )
        } else {
            root.reason.clone()
        };
        collector.add(
            kind,
            root.path.clone(),
            reason,
            Some(root.path.clone()),
            state_for_path(&root.path, path_states),
        );
    }

    for path in path_states.keys() {
        if !in_scope(path, scope_path) || is_excluded(repository_root, path, exclusions) {
            continue;
        }
        let base_name = basename(path).to_ascii_lowercase();
        let project_root = project_root_for_path(path, &roots);
        let state = state_for_path(path, path_states);

        if is_readme(&base_name) {
            collector.add(
                LandmarkKind::Readme,
                path.clone(),
                format!("recognized documentation filename `{base_name}`"),
                project_root.clone(),
                state,
            );
        }
        if is_contributor_instructions(&base_name) {
            collector.add(
                LandmarkKind::ContributorInstructions,
                path.clone(),
                format!("recognized contributor-documentation filename `{base_name}`"),
                project_root.clone(),
                state,
            );
        }
        if is_agent_instructions(&base_name) {
            collector.add(
                LandmarkKind::AgentInstructions,
                path.clone(),
                format!("recognized agent-instruction filename `{base_name}`"),
                project_root.clone(),
                state,
            );
        }
        if is_manifest(&base_name) {
            collector.add(
                LandmarkKind::Manifest,
                path.clone(),
                format!("recognized project manifest filename `{base_name}`"),
                project_root.clone(),
                state,
            );
        }
        if is_lockfile(&base_name) {
            collector.add(
                LandmarkKind::Lockfile,
                path.clone(),
                format!("recognized dependency-lock filename `{base_name}`"),
                project_root.clone(),
                state,
            );
        }
        if is_build_entry_point(&base_name) {
            collector.add(
                LandmarkKind::BuildEntryPoint,
                path.clone(),
                format!("recognized build-entry filename `{base_name}`"),
                project_root.clone(),
                state,
            );
        }
        if is_task_entry_point(&base_name) {
            collector.add(
                LandmarkKind::TaskEntryPoint,
                path.clone(),
                format!("recognized task-entry filename `{base_name}`"),
                project_root.clone(),
                state,
            );
        }
        if is_ci_file(path, &base_name) {
            collector.add(
                LandmarkKind::Ci,
                path.clone(),
                "recognized continuous-integration configuration path".to_owned(),
                project_root.clone(),
                state,
            );
        }
        if is_ownership_file(&base_name) {
            collector.add(
                LandmarkKind::Ownership,
                path.clone(),
                format!("recognized ownership filename `{base_name}`"),
                project_root.clone(),
                state,
            );
        }
        if is_license_file(&base_name) {
            collector.add(
                LandmarkKind::License,
                path.clone(),
                format!("recognized license or notice filename `{base_name}`"),
                project_root.clone(),
                state,
            );
        }
        if base_name == ".gitmodules" {
            collector.add(
                LandmarkKind::Submodule,
                path.clone(),
                "recognized Git submodule configuration filename".to_owned(),
                project_root.clone(),
                state,
            );
            let (declared_submodules, submodule_limitations) =
                detect_declared_submodules(repository_root, scope_root, path, exclusions, limits, scope_path);
            limitations.extend(submodule_limitations);
            for submodule_path in declared_submodules {
                collector.add(
                    LandmarkKind::Submodule,
                    submodule_path.clone(),
                    format!("submodule path declared by `{path}`"),
                    project_root_for_path(&submodule_path, &roots),
                    state_for_path(&submodule_path, path_states),
                );
            }
        }

        for directory in parent_directories(path) {
            let directory_name = basename(&directory).to_ascii_lowercase();
            if is_test_directory(&directory_name) {
                collector.add(
                    LandmarkKind::TestRoot,
                    directory.clone(),
                    format!("recognized test-root directory `{directory_name}`"),
                    project_root_for_path(&directory, &roots),
                    state_for_path(&directory, path_states),
                );
            }
            if is_ci_directory(&directory) {
                collector.add(
                    LandmarkKind::Ci,
                    directory.clone(),
                    "recognized continuous-integration directory".to_owned(),
                    project_root_for_path(&directory, &roots),
                    state_for_path(&directory, path_states),
                );
            }
        }
    }

    for path in submodule_paths {
        if in_scope(path, scope_path) && !is_excluded(repository_root, path, exclusions) {
            collector.add(
                LandmarkKind::Submodule,
                path.clone(),
                "Git HEAD tree entry uses the submodule (gitlink) mode".to_owned(),
                project_root_for_path(path, &roots),
                state_for_path(path, path_states),
            );
        }
    }

    let (nested, nested_limitations) = detect_nested_repositories(
        repository_root,
        scope_root,
        scope_path,
        exclusions,
        limits.max_files,
        recursive,
    );
    limitations.extend(nested_limitations);
    for path in nested {
        collector.add(
            LandmarkKind::NestedRepository,
            path.clone(),
            if recursive {
                "nested repository boundary detected; recursive analysis was explicitly requested".to_owned()
            } else {
                "nested repository boundary detected; traversal stops here by default".to_owned()
            },
            project_root_for_path(&path, &roots),
            state_for_path(&path, path_states),
        );
    }

    let mut landmarks = collector.into_landmarks();
    landmarks.sort_by(|left, right| {
        right
            .priority
            .cmp(&left.priority)
            .then_with(|| right.focus_matches.cmp(&left.focus_matches))
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.kind.label().cmp(right.kind.label()))
    });
    roots.sort_by(|left, right| left.path.cmp(&right.path));

    LandmarkAnalysis { landmarks, project_roots: roots, limitations }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ManifestRole {
    Workspace,
    Package,
    Unknown,
}

#[derive(Default)]
struct RootInfo {
    manifests: Vec<String>,
    families: BTreeSet<String>,
    roles: Vec<ManifestRole>,
}

fn detect_project_roots(
    repository_root: &Path, scope_root: &Path, scope_path: &str, path_states: &BTreeMap<String, WorktreeState>,
    exclusions: Option<&Gitignore>, limits: &ReportLimits, limitations: &mut Vec<String>,
) -> Vec<ProjectRoot> {
    let mut roots = BTreeMap::<String, RootInfo>::new();
    for path in path_states.keys() {
        let base_name = basename(path).to_ascii_lowercase();
        if !in_scope(path, scope_path) || is_excluded(repository_root, path, exclusions) || !is_manifest(&base_name) {
            continue;
        }
        let root = parent_path(path);
        if roots.len() >= limits.max_project_roots && !roots.contains_key(&root) {
            continue;
        }
        let role = inspect_manifest(
            repository_root,
            scope_root,
            path,
            &base_name,
            limits.max_file_bytes,
            limitations,
        );
        let entry = roots.entry(root).or_default();
        entry.manifests.push(path.clone());
        entry.families.insert(manifest_family(&base_name).to_owned());
        entry.roles.push(role);
    }

    roots
        .into_iter()
        .map(|(path, mut info)| {
            info.manifests.sort();
            let kind = if info.families.len() > 1 {
                ProjectRootKind::Mixed
            } else if info.roles.contains(&ManifestRole::Unknown) {
                ProjectRootKind::Unknown
            } else if info.roles.contains(&ManifestRole::Workspace) {
                ProjectRootKind::Workspace
            } else if info.roles.iter().all(|role| *role == ManifestRole::Package) {
                ProjectRootKind::Package
            } else {
                ProjectRootKind::Unknown
            };
            let reason = format!(
                "project root inferred from {} manifest(s): {}",
                info.manifests.len(),
                info.manifests.join(", ")
            );
            ProjectRoot {
                path,
                kind,
                reason,
                manifests: info.manifests,
                landmark_total: 0,
                recommendation_total: 0,
                recommended_paths: Vec::new(),
            }
        })
        .collect()
}

fn inspect_manifest(
    repository_root: &Path, scope_root: &Path, path: &str, basename: &str, max_bytes: usize,
    limitations: &mut Vec<String>,
) -> ManifestRole {
    let bytes = match security::read_worktree_file_limited(repository_root, scope_root, path, max_bytes) {
        Ok(bytes) => bytes,
        Err(error) => {
            limitations.push(format!(
                "Could not inspect manifest `{path}`: {error}. The manifest remains a bounded landmark."
            ));
            return ManifestRole::Unknown;
        }
    };
    let text = String::from_utf8_lossy(&bytes).to_ascii_lowercase();
    match basename {
        "cargo.toml" => {
            if text.contains("[workspace]") {
                ManifestRole::Workspace
            } else {
                ManifestRole::Package
            }
        }
        "package.json" => {
            if text.contains("\"workspaces\"") {
                ManifestRole::Workspace
            } else {
                ManifestRole::Package
            }
        }
        "pnpm-workspace.yaml" | "pnpm-workspace.yml" | "lerna.json" | "nx.json" => ManifestRole::Workspace,
        "pom.xml" | "settings.gradle" | "settings.gradle.kts" => {
            if text.contains("<modules>") || text.contains("include(") {
                ManifestRole::Workspace
            } else {
                ManifestRole::Package
            }
        }
        "pyproject.toml" | "setup.py" | "setup.cfg" | "gemfile" | "gemspec" | "build.gradle" | "build.gradle.kts"
        | "composer.json" | "mix.exs" => ManifestRole::Package,
        _ if basename.ends_with(".csproj") || basename.ends_with(".sln") => ManifestRole::Package,
        _ => ManifestRole::Unknown,
    }
}

fn detect_declared_submodules(
    repository_root: &Path, scope_root: &Path, gitmodules_path: &str, exclusions: Option<&Gitignore>,
    limits: &ReportLimits, scope_path: &str,
) -> (Vec<String>, Vec<String>) {
    let mut submodules = Vec::new();
    let mut limitations = Vec::new();
    let bytes =
        match security::read_worktree_file_limited(repository_root, scope_root, gitmodules_path, limits.max_file_bytes)
        {
            Ok(bytes) => bytes,
            Err(error) => {
                limitations.push(format!(
                    "Could not inspect submodule declarations in `{gitmodules_path}`: {error}."
                ));
                return (submodules, limitations);
            }
        };
    for line in String::from_utf8_lossy(&bytes).lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "path" {
            continue;
        }
        let value = value.trim();
        let Ok(path) = security::validate_repository_path(value.as_bytes()) else {
            limitations.push(format!(
                "Ignored unsafe submodule path `{value}` from `{gitmodules_path}`."
            ));
            continue;
        };
        if in_scope(&path, scope_path) && !is_excluded(repository_root, &path, exclusions) {
            submodules.push(path);
        }
    }
    (submodules, limitations)
}

fn detect_nested_repositories(
    repository_root: &Path, scope_root: &Path, scope_path: &str, exclusions: Option<&Gitignore>, max_entries: usize,
    recursive: bool,
) -> (Vec<String>, Vec<String>) {
    let mut builder = WalkBuilder::new(scope_root);
    builder
        .standard_filters(false)
        .hidden(false)
        .follow_links(false)
        .sort_by_file_path(|left, right| left.cmp(right));
    let root_for_filter = repository_root.to_owned();
    let nested = Arc::new(Mutex::new(BTreeSet::new()));
    let nested_for_filter = Arc::clone(&nested);
    let mut limitations = Vec::new();
    builder.filter_entry(move |entry| {
        if entry.depth() == 0 {
            return true;
        }
        let Some(file_type) = entry.file_type() else {
            return false;
        };
        if !file_type.is_dir() || entry.path_is_symlink() {
            return false;
        }
        let Some(relative) = entry.path().strip_prefix(&root_for_filter).ok() else {
            return false;
        };
        let Ok(relative) = security::validate_os_relative_path(relative) else {
            return false;
        };
        if is_nested_repository_directory(entry.path()) {
            if let Ok(mut nested) = nested_for_filter.lock() {
                nested.insert(relative);
            }
            return recursive;
        }
        !pruned_directory(entry.path())
    });

    for (visited_entries, item) in builder.build().enumerate() {
        let nested_limit_reached = nested.lock().map(|paths| paths.len() >= max_entries).unwrap_or(true);
        if visited_entries >= max_entries || nested_limit_reached {
            limitations.push(format!(
                "Nested-repository landmark scan reached the {max_entries}-entry traversal limit."
            ));
            break;
        }
        if let Err(error) = item {
            limitations.push(format!("Nested-repository landmark scan reported an error: {error}"));
        }
    }
    let nested = nested.lock().map(|paths| paths.clone()).unwrap_or_default();
    let nested = nested
        .into_iter()
        .filter(|path| in_scope(path, scope_path) && !is_excluded(repository_root, path, exclusions))
        .collect();
    (nested, limitations)
}

struct LandmarkCollector<'a> {
    landmarks: Vec<Landmark>,
    seen: BTreeSet<(LandmarkKind, String)>,
    focuses: &'a [String],
    focus_paths: &'a [String],
}

impl<'a> LandmarkCollector<'a> {
    fn new(focuses: &'a [String], focus_paths: &'a [String]) -> Self {
        Self { landmarks: Vec::new(), seen: BTreeSet::new(), focuses, focus_paths }
    }

    fn add(
        &mut self, kind: LandmarkKind, path: String, reason: String, project_root: Option<String>,
        worktree_state: WorktreeState,
    ) {
        if !self.seen.insert((kind, path.clone())) {
            return;
        }
        self.landmarks.push(Landmark {
            kind,
            path: path.clone(),
            reason,
            project_root,
            worktree_state,
            priority: kind.priority(),
            focus_matches: focus_matches(&path, kind, self.focuses, self.focus_paths),
        });
    }

    fn into_landmarks(self) -> Vec<Landmark> {
        self.landmarks
    }
}

fn focus_matches(path: &str, kind: LandmarkKind, focuses: &[String], focus_paths: &[String]) -> usize {
    let lower_path = path.to_ascii_lowercase();
    let text_matches = focuses
        .iter()
        .filter(|focus| {
            let focus = focus.trim().to_ascii_lowercase();
            !focus.is_empty() && (lower_path.contains(&focus) || kind.label().contains(&focus))
        })
        .count();
    let path_matches = focus_paths
        .iter()
        .filter(|focus_path| {
            let focus_path = focus_path.trim().trim_matches('/');
            !focus_path.is_empty() && (path == focus_path || path.starts_with(&format!("{focus_path}/")))
        })
        .count();
    text_matches + path_matches
}

pub fn project_root_for_path(path: &str, roots: &[ProjectRoot]) -> Option<String> {
    roots
        .iter()
        .filter(|root| root.path == "." || path == root.path || path.starts_with(&format!("{}/", root.path)))
        .max_by_key(|root| root.path.len())
        .map(|root| root.path.clone())
}

fn state_for_path(path: &str, states: &BTreeMap<String, WorktreeState>) -> WorktreeState {
    if let Some(state) = states.get(path) {
        return *state;
    }
    let prefix = if path == "." { String::new() } else { format!("{path}/") };
    let mut state = WorktreeState::Unknown;
    for descendant in states.keys().filter(|candidate| candidate.starts_with(&prefix)) {
        match states[descendant] {
            WorktreeState::Modified => return WorktreeState::Modified,
            WorktreeState::Untracked => state = WorktreeState::Untracked,
            WorktreeState::Tracked if state == WorktreeState::Unknown => state = WorktreeState::Tracked,
            WorktreeState::Unknown => {}
            WorktreeState::Tracked => {}
        }
    }
    state
}

fn is_excluded(repository_root: &Path, path: &str, exclusions: Option<&Gitignore>) -> bool {
    exclusions.is_some_and(|matcher| {
        matcher
            .matched_path_or_any_parents(repository_root.join(path), false)
            .is_ignore()
    })
}

fn in_scope(path: &str, scope: &str) -> bool {
    scope == "." || path == scope || path.starts_with(&format!("{scope}/"))
}

fn basename(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_owned()
}

fn parent_path(path: &str) -> String {
    path.rsplit_once('/')
        .map_or_else(|| ".".to_owned(), |(parent, _)| parent.to_owned())
}

fn parent_directories(path: &str) -> Vec<String> {
    let mut current = parent_path(path);
    let mut directories = Vec::new();
    while current != "." {
        directories.push(current.clone());
        current = parent_path(&current);
    }
    directories
}

fn is_readme(name: &str) -> bool {
    matches!(name, "readme" | "readme.md" | "readme.rst" | "readme.txt")
}

fn is_contributor_instructions(name: &str) -> bool {
    name.starts_with("contributing") || matches!(name, "code_of_conduct.md" | "community.md")
}

fn is_agent_instructions(name: &str) -> bool {
    matches!(name, "agents.md" | "claude.md" | "codex.md")
}

fn is_manifest(name: &str) -> bool {
    matches!(
        name,
        "cargo.toml"
            | "package.json"
            | "pyproject.toml"
            | "setup.py"
            | "setup.cfg"
            | "gemfile"
            | "gemspec"
            | "pom.xml"
            | "build.gradle"
            | "build.gradle.kts"
            | "settings.gradle"
            | "settings.gradle.kts"
            | "composer.json"
            | "mix.exs"
            | "pnpm-workspace.yaml"
            | "pnpm-workspace.yml"
            | "lerna.json"
            | "nx.json"
    ) || name.ends_with(".csproj")
        || name.ends_with(".sln")
}

fn manifest_family(name: &str) -> &str {
    if name == "cargo.toml" {
        "rust"
    } else if matches!(
        name,
        "package.json" | "pnpm-workspace.yaml" | "pnpm-workspace.yml" | "lerna.json" | "nx.json"
    ) {
        "node"
    } else if matches!(name, "pyproject.toml" | "setup.py" | "setup.cfg") {
        "python"
    } else if matches!(name, "gemfile" | "gemspec") {
        "ruby"
    } else if matches!(
        name,
        "pom.xml" | "build.gradle" | "build.gradle.kts" | "settings.gradle" | "settings.gradle.kts"
    ) {
        "jvm"
    } else if name.ends_with(".csproj") || name.ends_with(".sln") {
        "dotnet"
    } else {
        "other"
    }
}

fn is_lockfile(name: &str) -> bool {
    matches!(
        name,
        "cargo.lock"
            | "package-lock.json"
            | "pnpm-lock.yaml"
            | "yarn.lock"
            | "bun.lockb"
            | "poetry.lock"
            | "uv.lock"
            | "gemfile.lock"
            | "composer.lock"
            | "go.sum"
    )
}

fn is_build_entry_point(name: &str) -> bool {
    matches!(
        name,
        "build.rs" | "makefile" | "gnumakefile" | "pom.xml" | "build.gradle" | "build.gradle.kts"
    )
}

fn is_task_entry_point(name: &str) -> bool {
    matches!(
        name,
        "justfile" | "taskfile.yml" | "taskfile.yaml" | "rakefile" | "procfile"
    )
}

fn is_ci_file(path: &str, name: &str) -> bool {
    matches!(
        name,
        ".gitlab-ci.yml" | ".gitlab-ci.yaml" | "azure-pipelines.yml" | "azure-pipelines.yaml"
    ) || path
        .split('/')
        .any(|component| component.eq_ignore_ascii_case("workflows"))
        || path
            .split('/')
            .any(|component| component.eq_ignore_ascii_case(".circleci"))
        || path
            .split('/')
            .any(|component| component.eq_ignore_ascii_case(".buildkite"))
}

fn is_ci_directory(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower == ".github/workflows" || lower == ".circleci" || lower == ".buildkite"
}

fn is_ownership_file(name: &str) -> bool {
    matches!(name, "codeowners" | "owners" | "maintainers")
}

fn is_license_file(name: &str) -> bool {
    name.starts_with("license") || name.starts_with("copying") || name.starts_with("notice")
}

fn is_test_directory(name: &str) -> bool {
    matches!(name, "test" | "tests" | "spec" | "__tests__")
}

fn is_nested_repository_directory(path: &Path) -> bool {
    fs::symlink_metadata(path.join(".git")).is_ok_and(|metadata| metadata.is_dir() || metadata.is_file())
}

fn pruned_directory(path: &Path) -> bool {
    path.file_name().and_then(|name| name.to_str()).is_some_and(|name| {
        matches!(
            name,
            ".git" | "target" | "node_modules" | "vendor" | "dist" | "build" | "out" | "coverage"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_landmark_filenames_and_project_roots() {
        assert!(is_readme("readme.md"));
        assert!(is_agent_instructions("agents.md"));
        assert!(is_manifest("cargo.toml"));
        assert!(is_lockfile("pnpm-lock.yaml"));
        assert!(is_test_directory("__tests__"));
        assert_eq!(manifest_family("package.json"), "node");
    }

    #[test]
    fn project_root_matching_prefers_the_deepest_root() {
        let roots = vec![
            ProjectRoot {
                path: ".".to_owned(),
                kind: ProjectRootKind::Workspace,
                reason: String::new(),
                manifests: Vec::new(),
                landmark_total: 0,
                recommendation_total: 0,
                recommended_paths: Vec::new(),
            },
            ProjectRoot {
                path: "packages/app".to_owned(),
                kind: ProjectRootKind::Package,
                reason: String::new(),
                manifests: Vec::new(),
                landmark_total: 0,
                recommendation_total: 0,
                recommended_paths: Vec::new(),
            },
        ];
        assert_eq!(
            project_root_for_path("packages/app/src/lib.rs", &roots).as_deref(),
            Some("packages/app")
        );
        assert_eq!(project_root_for_path("README.md", &roots).as_deref(), Some("."));
    }

    #[test]
    fn detects_workspace_manifest_root() {
        let root = std::env::temp_dir().join(format!("codeplat-landmark-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create landmark test root");
        std::fs::write(root.join("Cargo.toml"), "[workspace]\n").expect("write landmark manifest");
        let states = [("Cargo.toml".to_owned(), WorktreeState::Tracked)]
            .into_iter()
            .collect::<BTreeMap<_, _>>();
        let roots = detect_project_roots(
            &root,
            &root,
            ".",
            &states,
            None,
            &ReportLimits::default(),
            &mut Vec::new(),
        );
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].kind, ProjectRootKind::Workspace);
        let _ = std::fs::remove_dir_all(root);
    }
}
