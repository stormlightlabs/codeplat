use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use gix::bstr::ByteSlice;
use gix::revision::walk::{Sorting, iter::Error as WalkIterError};
use gix::traverse::commit::simple::CommitTimeOrder;

use crate::cli::ExitCategory;
use crate::report::{
    ActivityReport, BugReport, ChurnReport, CommitEvidence, ContributorCount, ContributorReport, FirefightingReport,
    HistoryOperation, HistoryReport, HistorySettings, MonthlyActivity, PathCount,
};
use crate::utils;

const CHURN_CAVEAT: &str =
    "Absolute churn is not normalized by file size; active development is not automatically risky.";
const CONTRIBUTOR_CAVEAT: &str =
    "Squash merges can credit a merger rather than the original author; commit count is only a knowledge proxy.";
const BUG_CAVEAT: &str = "Bug clusters depend on commit-message discipline and do not prove a defect rate.";
const ACTIVITY_CAVEAT: &str = "Cadence reflects team and release habits, not just repository health.";
const FIREFIGHTING_CAVEAT: &str =
    "Firefighting matches are keyword evidence, not a complete measure of release health.";

#[derive(Debug, thiserror::Error)]
pub enum HistoryError {
    #[error("could not discover a Git repository from `{path}`: {source}")]
    Discovery {
        path: PathBuf,
        #[source]
        source: Box<gix::discover::Error>,
    },
    #[error("history input `{path}` is invalid: {reason}")]
    Input { path: PathBuf, reason: String },
    #[error("history analysis failed while {operation}: {reason}")]
    Analysis { operation: &'static str, reason: String },
}

impl Into<ExitCategory> for HistoryError {
    fn into(self) -> ExitCategory {
        match self {
            Self::Discovery { .. } => ExitCategory::Repository,
            Self::Input { .. } => ExitCategory::Input,
            Self::Analysis { .. } => ExitCategory::Analysis,
        }
    }
}

impl HistoryError {
    fn analysis(operation: &'static str, error: impl std::fmt::Display) -> Self {
        Self::Analysis { operation, reason: error.to_string() }
    }
}

#[derive(Debug)]
struct Scope {
    repository_root: PathBuf,
    relative_path: String,
}

#[derive(Debug)]
struct CommitRecord {
    id: String,
    subject: String,
    message: String,
    author_name: String,
    author_email: String,
    author_seconds: i64,
    committer_seconds: i64,
    is_merge: bool,
    paths: Vec<String>,
}

impl CommitRecord {
    fn evidence(&self, paths: Vec<String>) -> CommitEvidence {
        CommitEvidence { id: self.id.clone(), subject: self.subject.clone(), paths }
    }
}

pub fn analyze(
    path: &Path, settings: HistorySettings, operation: Option<HistoryOperation>,
) -> Result<HistoryReport, HistoryError> {
    if settings.window_days == 0 || settings.recent_window_days == 0 {
        return Err(HistoryError::Input {
            path: path.to_owned(),
            reason: "time windows must be greater than zero".to_owned(),
        });
    }

    let selected_path = absolute_path(path)?;
    let repository = gix::discover(&selected_path)
        .map_err(|source| HistoryError::Discovery { path: selected_path.clone(), source: Box::new(source) })?;
    let scope = resolve_scope(&repository, &selected_path)?;
    let head = repository
        .head_id()
        .map_err(|error| HistoryError::analysis("resolving HEAD", error))?;
    let records = collect_commits(&repository, head)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| HistoryError::analysis("reading the current time", error))?
        .as_secs() as i64;

    let include = |candidate| operation.is_none_or(|selected| selected == candidate);
    let churn_analysis = analyze_churn(&records, &scope.relative_path, &settings, now);
    let churn = include(HistoryOperation::Churn).then_some(churn_analysis.clone());
    let contributors = include(HistoryOperation::Contributors)
        .then(|| analyze_contributors(&records, &scope.relative_path, &settings, now));
    let bugs = include(HistoryOperation::Bugs)
        .then(|| analyze_bugs(&records, &scope.relative_path, &settings, now, &churn_analysis.paths));
    let activity = include(HistoryOperation::Activity).then(|| analyze_activity(&records));
    let firefighting = include(HistoryOperation::Firefighting)
        .then(|| analyze_firefighting(&records, &scope.relative_path, &settings, now));

    let non_merge_commits_seen = records.iter().filter(|record| !record.is_merge).count();
    Ok(HistoryReport {
        repository_root: scope.repository_root.to_string_lossy().into_owned(),
        scope_path: scope.relative_path,
        settings,
        commits_seen: records.len(),
        non_merge_commits_seen,
        churn,
        contributors,
        bugs,
        activity,
        firefighting,
    })
}

fn absolute_path(path: &Path) -> Result<PathBuf, HistoryError> {
    let path = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()
            .map_err(|error| HistoryError::analysis("reading the current directory", error))?
            .join(path)
    };
    std::fs::canonicalize(&path).map_err(|error| HistoryError::Input { path, reason: error.to_string() })
}

fn resolve_scope(repository: &gix::Repository, selected_path: &Path) -> Result<Scope, HistoryError> {
    let repository_root = repository.workdir().ok_or_else(|| HistoryError::Input {
        path: selected_path.to_owned(),
        reason: "the discovered repository has no worktree".to_owned(),
    })?;
    let repository_root = std::fs::canonicalize(repository_root)
        .map_err(|error| HistoryError::Input { path: repository_root.to_owned(), reason: error.to_string() })?;
    let relative = selected_path
        .strip_prefix(&repository_root)
        .map_err(|_| HistoryError::Input {
            path: selected_path.to_owned(),
            reason: format!("path is outside repository `{}`", repository_root.display()),
        })?;
    let relative_path = if relative.as_os_str().is_empty() {
        ".".to_owned()
    } else {
        relative.to_string_lossy().replace('\\', "/")
    };
    Ok(Scope { repository_root, relative_path })
}

fn collect_commits(repository: &gix::Repository, head: gix::Id<'_>) -> Result<Vec<CommitRecord>, HistoryError> {
    let walk = repository
        .rev_walk([head])
        .sorting(Sorting::ByCommitTime(CommitTimeOrder::NewestFirst))
        .all()
        .map_err(|error| HistoryError::analysis("walking revisions", error))?;
    let mut records = Vec::new();
    for info in walk {
        let info = info.map_err(|error: WalkIterError| HistoryError::analysis("walking revisions", error))?;
        let id = info.id;
        let commit = info
            .object()
            .map_err(|error| HistoryError::analysis("reading a commit object", error))?;
        let author = commit
            .author()
            .map_err(|error| HistoryError::analysis("decoding a commit author", error))?;
        let author_time = author
            .time()
            .map_err(|error| HistoryError::analysis("decoding an author timestamp", error))?;
        let committer = commit
            .committer()
            .map_err(|error| HistoryError::analysis("decoding a commit committer", error))?;
        let committer_time = committer
            .time()
            .map_err(|error| HistoryError::analysis("decoding a committer timestamp", error))?;
        let subject = commit
            .message()
            .map_err(|error| HistoryError::analysis("decoding a commit message", error))?
            .summary();
        let message = commit
            .message_raw()
            .map_err(|error| HistoryError::analysis("decoding a commit message", error))?
            .to_str_lossy()
            .into_owned();
        let parents: Vec<_> = commit.parent_ids().collect();
        let paths = if parents.len() <= 1 {
            changed_paths(repository, &commit, parents.first().copied())?
        } else {
            Vec::new()
        };

        records.push(CommitRecord {
            id: id.to_string(),
            subject: String::from_utf8_lossy(subject.as_ref()).trim().to_owned(),
            message,
            author_name: author.name.to_str_lossy().into_owned(),
            author_email: author.email.to_str_lossy().into_owned(),
            author_seconds: author_time.seconds,
            committer_seconds: committer_time.seconds,
            is_merge: parents.len() > 1,
            paths,
        });
    }
    Ok(records)
}

fn changed_paths(
    repository: &gix::Repository, commit: &gix::Commit<'_>, parent: Option<gix::Id<'_>>,
) -> Result<Vec<String>, HistoryError> {
    let current_tree = commit
        .tree()
        .map_err(|error| HistoryError::analysis("reading a commit tree", error))?;
    let previous_tree = match parent {
        Some(parent) => {
            let parent_commit = repository
                .find_commit(parent)
                .map_err(|error| HistoryError::analysis("reading a parent commit", error))?;
            parent_commit
                .tree()
                .map_err(|error| HistoryError::analysis("reading a parent tree", error))?
        }
        None => repository.empty_tree(),
    };
    let mut paths = BTreeSet::new();
    collect_tree_changes(repository, &previous_tree, &current_tree, "", &mut paths)?;
    Ok(paths.into_iter().collect())
}

fn collect_tree_changes(
    repository: &gix::Repository, previous: &gix::Tree<'_>, current: &gix::Tree<'_>, prefix: &str,
    changed: &mut BTreeSet<String>,
) -> Result<(), HistoryError> {
    if previous.id == current.id {
        return Ok(());
    }
    let previous_entries = tree_entries(previous)?;
    let current_entries = tree_entries(current)?;
    let names: BTreeSet<_> = previous_entries.keys().chain(current_entries.keys()).cloned().collect();
    for name in names {
        let path = if prefix.is_empty() { name.clone() } else { format!("{prefix}/{name}") };
        match (previous_entries.get(&name), current_entries.get(&name)) {
            (Some((previous_mode, previous_id)), Some((current_mode, current_id)))
                if previous_mode.is_tree() && current_mode.is_tree() =>
            {
                if previous_id != current_id {
                    let previous_tree = repository
                        .find_tree(*previous_id)
                        .map_err(|error| HistoryError::analysis("reading a previous directory tree", error))?;
                    let current_tree = repository
                        .find_tree(*current_id)
                        .map_err(|error| HistoryError::analysis("reading a current directory tree", error))?;
                    collect_tree_changes(repository, &previous_tree, &current_tree, &path, changed)?;
                }
            }
            (Some((previous_mode, previous_id)), Some((current_mode, current_id)))
                if previous_mode == current_mode && previous_id == current_id => {}
            (Some((previous_mode, previous_id)), Some((current_mode, current_id))) => {
                collect_changed_entry(repository, *previous_mode, *previous_id, &path, changed)?;
                collect_changed_entry(repository, *current_mode, *current_id, &path, changed)?;
            }
            (Some((mode, id)), None) | (None, Some((mode, id))) => {
                collect_changed_entry(repository, *mode, *id, &path, changed)?;
            }
            (None, None) => continue,
        }
    }
    Ok(())
}

fn tree_entries(
    tree: &gix::Tree<'_>,
) -> Result<BTreeMap<String, (gix::objs::tree::EntryMode, gix::ObjectId)>, HistoryError> {
    let decoded = gix::objs::TreeRef::from_bytes(&tree.data, tree.id.kind())
        .map_err(|error| HistoryError::analysis("decoding a directory tree", error))?;
    Ok(decoded
        .entries
        .into_iter()
        .map(|entry| {
            (
                entry.filename.to_str_lossy().into_owned(),
                (entry.mode, entry.oid.to_owned()),
            )
        })
        .collect())
}

fn collect_changed_entry(
    repository: &gix::Repository, mode: gix::objs::tree::EntryMode, id: gix::ObjectId, path: &str,
    changed: &mut BTreeSet<String>,
) -> Result<(), HistoryError> {
    if mode.is_tree() {
        let tree = repository
            .find_tree(id)
            .map_err(|error| HistoryError::analysis("reading a changed directory tree", error))?;
        for (name, (mode, id)) in tree_entries(&tree)? {
            let child_path = format!("{path}/{name}");
            collect_changed_entry(repository, mode, id, &child_path, changed)?;
        }
    } else {
        changed.insert(path.to_owned());
    }
    Ok(())
}

fn analyze_churn(records: &[CommitRecord], scope: &str, settings: &HistorySettings, now: i64) -> ChurnReport {
    let mut counts = BTreeMap::new();
    for record in records
        .iter()
        .filter(|record| !record.is_merge && utils::in_window(record.committer_seconds, now, settings.window_days))
    {
        for path in scoped_paths(&record.paths, scope) {
            *counts.entry(path).or_insert(0) += 1;
        }
    }
    ChurnReport {
        window_days: settings.window_days,
        paths: path_counts(counts),
        caveats: vec![CHURN_CAVEAT.to_owned()],
    }
}

fn analyze_contributors(
    records: &[CommitRecord], scope: &str, settings: &HistorySettings, now: i64,
) -> ContributorReport {
    let non_merge: Vec<_> = records
        .iter()
        .filter(|record| !record.is_merge && !scoped_paths(&record.paths, scope).is_empty())
        .collect();
    let recent: Vec<_> = non_merge
        .iter()
        .copied()
        .filter(|record| utils::in_window(record.author_seconds, now, settings.recent_window_days))
        .collect();
    ContributorReport {
        recent_window_days: settings.recent_window_days,
        overall: contributor_counts(non_merge),
        recent: contributor_counts(recent),
        caveats: vec![CONTRIBUTOR_CAVEAT.to_owned()],
    }
}

fn contributor_counts(records: Vec<&CommitRecord>) -> Vec<ContributorCount> {
    let total = records.len();
    let mut counts = BTreeMap::<String, (String, String, usize)>::new();
    for record in records {
        let key = if record.author_email.is_empty() {
            record.author_name.clone()
        } else {
            record.author_email.clone()
        };
        let entry = counts
            .entry(key)
            .or_insert_with(|| (record.author_name.clone(), record.author_email.clone(), 0));
        entry.2 += 1;
    }
    let mut contributors: Vec<_> = counts
        .into_values()
        .map(|(name, email, commits)| ContributorCount {
            name,
            email,
            commits,
            share_percent: (commits.saturating_mul(100).checked_div(total.max(1)).unwrap_or(0)) as u8,
        })
        .collect();
    contributors.sort_by(|left, right| {
        right
            .commits
            .cmp(&left.commits)
            .then_with(|| left.email.cmp(&right.email))
            .then_with(|| left.name.cmp(&right.name))
    });
    contributors
}

fn analyze_bugs(
    records: &[CommitRecord], scope: &str, settings: &HistorySettings, now: i64, churn_paths: &[PathCount],
) -> BugReport {
    let mut counts = BTreeMap::new();
    let mut commits = Vec::new();
    for record in records.iter().filter(|record| {
        !record.is_merge
            && utils::in_window(record.committer_seconds, now, settings.window_days)
            && utils::contains_keyword(&record.message, &settings.bug_keywords)
    }) {
        let paths = scoped_paths(&record.paths, scope);
        if paths.is_empty() {
            continue;
        }
        for path in &paths {
            *counts.entry(path.clone()).or_insert(0) += 1;
        }
        commits.push(record.evidence(paths));
    }
    let paths = path_counts(counts);
    let churn_paths: BTreeSet<_> = churn_paths.iter().map(|path| path.path.as_str()).collect();
    let overlap_paths = paths
        .iter()
        .filter(|path| churn_paths.contains(path.path.as_str()))
        .cloned()
        .collect();
    let mut caveats = vec![BUG_CAVEAT.to_owned()];
    if commits.is_empty() {
        caveats.push(
            "No bug-related commits matched; this can mean stability or vague commit messages, not proof of quality."
                .to_owned(),
        );
    }
    BugReport {
        window_days: settings.window_days,
        keywords: settings.bug_keywords.clone(),
        paths,
        overlap_paths,
        commits,
        caveats,
    }
}

fn analyze_activity(records: &[CommitRecord]) -> ActivityReport {
    let mut months = BTreeMap::new();
    for record in records {
        *months
            .entry(utils::month_for_timestamp(record.author_seconds))
            .or_insert(0) += 1;
    }
    ActivityReport {
        months: months
            .into_iter()
            .map(|(month, commits)| MonthlyActivity { month, commits })
            .collect(),
        caveats: vec![ACTIVITY_CAVEAT.to_owned()],
    }
}

fn analyze_firefighting(
    records: &[CommitRecord], scope: &str, settings: &HistorySettings, now: i64,
) -> FirefightingReport {
    let commits = records
        .iter()
        .filter(|record| {
            !record.is_merge
                && utils::in_window(record.committer_seconds, now, settings.window_days)
                && utils::contains_keyword(&record.message, &settings.firefighting_keywords)
        })
        .filter_map(|record| {
            let paths = scoped_paths(&record.paths, scope);
            (!paths.is_empty()).then(|| record.evidence(paths))
        })
        .collect::<Vec<_>>();
    let mut caveats = vec![FIREFIGHTING_CAVEAT.to_owned()];
    if commits.is_empty() {
        caveats.push(
            "No firefighting-language commits matched; this can mean stability or vague commit messages, not proof of quality."
                .to_owned(),
        );
    }
    FirefightingReport {
        window_days: settings.window_days,
        keywords: settings.firefighting_keywords.clone(),
        commits,
        caveats,
    }
}

fn scoped_paths(paths: &[String], scope: &str) -> Vec<String> {
    if scope == "." {
        return paths.to_vec();
    }
    paths
        .iter()
        .filter(|path| *path == scope || path.starts_with(&format!("{scope}/")))
        .cloned()
        .collect()
}

fn path_counts(counts: BTreeMap<String, usize>) -> Vec<PathCount> {
    let mut paths: Vec<_> = counts
        .into_iter()
        .map(|(path, commits)| PathCount { path, commits })
        .collect();
    paths.sort_by(|left, right| {
        right
            .commits
            .cmp(&left.commits)
            .then_with(|| left.path.cmp(&right.path))
    });
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_matching_includes_descendants_but_not_siblings() {
        let paths = vec![
            "src/lib.rs".to_owned(),
            "src/bin/main.rs".to_owned(),
            "tests/lib.rs".to_owned(),
        ];
        assert_eq!(scoped_paths(&paths, "src"), vec!["src/lib.rs", "src/bin/main.rs"]);
        assert_eq!(scoped_paths(&paths, "src/lib.rs"), vec!["src/lib.rs"]);
    }
}
