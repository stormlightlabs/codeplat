use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use gix::bstr::ByteSlice;
use gix::revision::walk::Sorting;
use gix::traverse::commit::simple::CommitTimeOrder;

use crate::cli::ExitCategory;
use crate::report::*;
use crate::security;
use crate::utils;

const CHURN_CAVEAT: &str =
    "Absolute churn is not normalized by file size & active development is not automatically risky.";

const CONTRIBUTOR_CAVEAT: &str =
    "Squash merges can credit a merger rather than the original author so commit count is only a knowledge proxy.";

const BUG_CAVEAT: &str = "Bug clusters depend on commit-message discipline and do not prove a defect rate.";
const ACTIVITY_CAVEAT: &str = "Cadence reflects team and release habits.";

const FIREFIGHTING_CAVEAT: &str =
    "Firefighting matches are keyword evidence and not a complete measure of release health.";

const MAX_CHANGED_PATHS_PER_COMMIT: usize = 10_000;
const MAX_TREE_NODES_PER_COMMIT: usize = 100_000;
const MAX_TREE_ENTRIES_PER_DIRECTORY: usize = 4_096;

type Result<T> = std::result::Result<T, HistoryError>;
type TreeEntryMap = BTreeMap<String, (gix::objs::tree::EntryMode, gix::ObjectId)>;

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
    #[error("history safety check failed while {operation}: {reason}")]
    Safety { operation: &'static str, reason: String },
}

impl From<HistoryError> for ExitCategory {
    fn from(error: HistoryError) -> Self {
        match error {
            HistoryError::Discovery { .. } => ExitCategory::Repository,
            HistoryError::Input { .. } => ExitCategory::Input,
            HistoryError::Analysis { .. } => ExitCategory::Analysis,
            HistoryError::Safety { .. } => ExitCategory::Input,
        }
    }
}

impl From<&HistoryError> for ExitCategory {
    fn from(error: &HistoryError) -> Self {
        match error {
            HistoryError::Discovery { .. } => ExitCategory::Repository,
            HistoryError::Input { .. } => ExitCategory::Input,
            HistoryError::Analysis { .. } => ExitCategory::Analysis,
            HistoryError::Safety { .. } => ExitCategory::Input,
        }
    }
}

impl HistoryError {
    fn analysis(operation: &'static str, error: impl std::fmt::Display) -> Self {
        Self::Analysis { operation, reason: error.to_string() }
    }

    fn safety(operation: &'static str, error: impl std::fmt::Display) -> Self {
        Self::Safety { operation, reason: error.to_string() }
    }
}

#[derive(Debug)]
struct CommitRecord {
    id: String,
    subject: String,
    message: String,
    author_name: String,
    author_email: String,
    raw_author_name: String,
    raw_author_email: String,
    author_seconds: i64,
    committer_seconds: i64,
    is_merge: bool,
    paths: Vec<String>,
}

struct CommitScan {
    records: Vec<CommitRecord>,
    commits_seen: usize,
    non_merge_commits_seen: usize,
    truncated: bool,
    elapsed_limited: bool,
    missing_objects: usize,
}

struct ChangedPaths {
    paths: Vec<String>,
    truncated: bool,
}

impl CommitRecord {
    fn evidence(&self, paths: Vec<String>, matched_terms: Vec<String>) -> CommitEvidence {
        CommitEvidence { id: self.id.clone(), subject: self.subject.clone(), paths, matched_terms }
    }

    fn affects_scope(&self, scope: &str) -> bool {
        scope == "." || !scoped_paths(&self.paths, scope).is_empty()
    }
}

pub fn analyze(
    path: &Path, settings: HistorySettings, operation: Option<HistoryOperation>, profile: AnalysisProfile,
) -> Result<HistoryReport> {
    if settings.window_days == 0 || settings.recent_window_days == 0 {
        return Err(HistoryError::Input {
            path: path.to_owned(),
            reason: "time windows must be greater than zero".to_owned(),
        });
    }

    let selected_path = absolute_path(path)?;
    let repository = security::discover_repository(&selected_path)
        .map_err(|source| HistoryError::Discovery { path: selected_path.clone(), source })?;
    let scope = security::resolve_scope(&repository, &selected_path).map_err(|error| match error {
        security::ScopeError::Input(reason) => HistoryError::Input { path: selected_path.clone(), reason },
        security::ScopeError::Safety(error) => HistoryError::safety("resolving the analysis scope", error),
    })?;
    let head = repository
        .head_id()
        .map_err(|error| HistoryError::analysis("resolving HEAD", error))?;
    let head_reference = repository
        .head_name()
        .map_err(|error| HistoryError::analysis("resolving HEAD reference", error))?
        .map(|name| name.as_bstr().to_str_lossy().into_owned());
    let reference_seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| HistoryError::analysis("reading the current time", error))?
        .as_secs() as i64;
    let limits = ReportLimits::for_profile(profile);
    let mut scan = collect_commits(&repository, head, &scope.relative_path, operation, &limits)?;
    let needs_contributors = operation.is_none_or(|selected| selected == HistoryOperation::Contributors);
    let mailmap = if needs_contributors { committed_mailmap(&repository, head)? } else { None };
    apply_mailmap(&mut scan.records, mailmap.as_ref());
    let records = std::mem::take(&mut scan.records);
    let now = reference_seconds;

    let include = |candidate| operation.is_none_or(|selected| selected == candidate);
    let include_churn = include(HistoryOperation::Churn);
    let mut churn_analysis = analyze_churn(&records, &scope.relative_path, &settings, now);
    if include_churn {
        add_head_sizes(&repository, head, &mut churn_analysis.paths, &limits)?;
    }
    let churn = include_churn.then_some(churn_analysis.clone());
    let contributors = include(HistoryOperation::Contributors)
        .then(|| analyze_contributors(&records, &scope.relative_path, &settings, now, mailmap.is_some()));
    let bugs = include(HistoryOperation::Bugs)
        .then(|| analyze_bugs(&records, &scope.relative_path, &settings, now, &churn_analysis.paths));

    let activity = include(HistoryOperation::Activity).then(|| analyze_activity(&records, &scope.relative_path));
    let firefighting = include(HistoryOperation::Firefighting)
        .then(|| analyze_firefighting(&records, &scope.relative_path, &settings, now));
    let commits_seen = scan.commits_seen;
    let non_merge_commits_seen = scan.non_merge_commits_seen;
    let head_snapshot = HeadSnapshot {
        reference: head_reference.clone(),
        oid: Some(head.to_string()),
        detached: head_reference.is_none(),
        unborn: false,
    };
    let completeness = history_completeness(&repository, &scan);
    let provenance = history_provenance(&records, &head_snapshot, completeness);
    let mut limitations = Vec::new();
    if scan.truncated {
        limitations.push(format!(
            "Reachable commit evidence was bounded at {}; aggregate counts include only the retained prefix.",
            limits.max_commits
        ));
    }
    if scan.elapsed_limited {
        limitations.push(format!(
            "History analysis stopped at the {} ms elapsed-work limit; the returned report is partial.",
            limits.max_elapsed_ms
        ));
    }
    let mut report = HistoryReport {
        repository_root: scope.repository_root.to_string_lossy().into_owned(),
        scope_path: scope.relative_path,
        head: head_snapshot,
        provenance,
        settings,
        commits_seen,
        non_merge_commits_seen,
        collections: HistoryCollections {
            commits: if scan.truncated {
                CollectionSummary::bounded(commits_seen, records.len(), TruncationReason::ResourceLimit)
            } else {
                CollectionSummary::complete(commits_seen)
            },
            churn_paths: CollectionSummary::complete(0),
            contributor_identity_mappings: CollectionSummary::complete(0),
            contributors_overall: CollectionSummary::complete(0),
            contributors_recent: CollectionSummary::complete(0),
            bug_paths: CollectionSummary::complete(0),
            bug_overlap_paths: CollectionSummary::complete(0),
            bug_commits: CollectionSummary::complete(0),
            activity_months: CollectionSummary::complete(0),
            firefighting_commits: CollectionSummary::complete(0),
        },
        limitations,
        churn,
        contributors,
        bugs,
        activity,
        firefighting,
    };
    bound_history(&mut report, ReportLimits::for_profile(profile));
    Ok(report)
}

fn bound_history(report: &mut HistoryReport, limits: ReportLimits) {
    let mut truncated = false;
    if let Some(churn) = &mut report.churn {
        report.collections.churn_paths = truncate(&mut churn.paths, limits.max_history_evidence);
        truncated |= report.collections.churn_paths.truncated;
    }
    if let Some(contributors) = &mut report.contributors {
        report.collections.contributor_identity_mappings =
            truncate(&mut contributors.identity_mappings, limits.max_history_evidence);
        report.collections.contributors_overall = truncate(&mut contributors.overall, limits.max_history_evidence);
        report.collections.contributors_recent = truncate(&mut contributors.recent, limits.max_history_evidence);
        truncated |= report.collections.contributor_identity_mappings.truncated
            || report.collections.contributors_overall.truncated
            || report.collections.contributors_recent.truncated;
    }
    if let Some(bugs) = &mut report.bugs {
        report.collections.bug_paths = truncate(&mut bugs.paths, limits.max_history_evidence);
        report.collections.bug_overlap_paths = truncate(&mut bugs.overlap_paths, limits.max_history_evidence);
        report.collections.bug_commits = truncate(&mut bugs.commits, limits.max_history_evidence);
        truncated |= report.collections.bug_paths.truncated
            || report.collections.bug_overlap_paths.truncated
            || report.collections.bug_commits.truncated;
    }
    if let Some(activity) = &mut report.activity {
        report.collections.activity_months = truncate(&mut activity.months, limits.max_history_evidence);
        truncated |= report.collections.activity_months.truncated;
    }
    if let Some(firefighting) = &mut report.firefighting {
        report.collections.firefighting_commits = truncate(&mut firefighting.commits, limits.max_history_evidence);
        truncated |= report.collections.firefighting_commits.truncated;
    }
    if truncated {
        report.limitations.push(format!(
            "History evidence was bounded to {} returned items per collection; use `--profile evidence` for the larger bounded evidence profile.",
            limits.max_history_evidence
        ));
    }
}

fn truncate<T>(values: &mut Vec<T>, limit: usize) -> CollectionSummary {
    let total = values.len();
    values.truncate(limit);
    CollectionSummary::bounded(total, values.len(), TruncationReason::CollectionLimit)
}

fn history_completeness(repository: &gix::Repository, scan: &CommitScan) -> HistoryCompleteness {
    let shallow = repository.is_shallow();
    let status = if scan.missing_objects > 0 {
        HistoryCompletenessStatus::MissingObjects
    } else if shallow {
        HistoryCompletenessStatus::Shallow
    } else if scan.truncated {
        HistoryCompletenessStatus::Partial
    } else {
        HistoryCompletenessStatus::Complete
    };
    let mut notes = Vec::new();
    if shallow {
        notes.push("The repository has a shallow boundary; commits before it were not reachable.".to_owned());
    }
    if scan.missing_objects > 0 {
        notes.push(format!(
            "{} reachable Git object(s) could not be read.",
            scan.missing_objects
        ));
    }
    if scan.truncated {
        notes.push("The commit scan was bounded by a resource or elapsed-work limit.".to_owned());
    }
    HistoryCompleteness {
        status,
        authoritative: matches!(status, HistoryCompletenessStatus::Complete),
        shallow,
        missing_objects: scan.missing_objects,
        notes,
    }
}

fn history_provenance(
    records: &[CommitRecord], head: &HeadSnapshot, completeness: HistoryCompleteness,
) -> HistoryProvenance {
    let mut timestamps = records.iter().map(|record| record.committer_seconds);
    let start = timestamps.next().map(|first| {
        records
            .iter()
            .map(|record| record.committer_seconds)
            .min()
            .unwrap_or(first)
    });
    let end = records.iter().map(|record| record.committer_seconds).max();
    HistoryProvenance {
        observed_date_range: ObservedDateRange {
            start: start.map(utils::timestamp_to_rfc3339_seconds),
            end: end.map(utils::timestamp_to_rfc3339_seconds),
            basis: "committer_date".to_owned(),
        },
        time_basis: HistoryTimeBasis {
            window_filters: "committer_date".to_owned(),
            contributor_recent_and_activity: "author_date".to_owned(),
        },
        current_head: CurrentHeadSemantics {
            meaning: "The report walks commits reachable from the current HEAD resolved at capture time; it does not imply an unbounded or complete history.".to_owned(),
            reference: head.reference.clone(),
            oid: head.oid.clone(),
        },
        completeness,
    }
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
        size_basis: "current_head_blob_bytes".to_owned(),
        rename_continuity: RenameContinuity {
            status: "unavailable".to_owned(),
            detail: "Rename detection is not implemented; counts follow exact paths, and a renamed or deleted path may have earlier history under another name.".to_owned(),
        },
        paths: path_counts(counts),
        caveats: vec![
            CHURN_CAVEAT.to_owned(),
            "Normalized churn uses the current HEAD blob size. Empty, binary, deleted, oversized, and resource-limited paths have no normalized rate; generated files are retained and labelled.".to_owned(),
        ],
    }
}

fn analyze_contributors(
    records: &[CommitRecord], scope: &str, settings: &HistorySettings, now: i64, mailmap_applied: bool,
) -> ContributorReport {
    let non_merge: Vec<_> = records
        .iter()
        .filter(|record| !record.is_merge && record.affects_scope(scope))
        .collect();
    let recent: Vec<_> = non_merge
        .iter()
        .copied()
        .filter(|record| utils::in_window(record.author_seconds, now, settings.recent_window_days))
        .collect();
    ContributorReport {
        recent_window_days: settings.recent_window_days,
        mailmap_applied,
        identity_mappings: identity_mappings(&non_merge, settings.include_emails),
        overall: contributor_counts(non_merge, settings.include_emails),
        recent: contributor_counts(recent, settings.include_emails),
        caveats: vec![CONTRIBUTOR_CAVEAT.to_owned()],
    }
}

fn analyze_bugs(
    records: &[CommitRecord], scope: &str, settings: &HistorySettings, now: i64, churn_paths: &[PathCount],
) -> BugReport {
    let mut counts = BTreeMap::new();
    let mut commits = Vec::new();
    for record in records
        .iter()
        .filter(|record| !record.is_merge && utils::in_window(record.committer_seconds, now, settings.window_days))
    {
        let matched_terms = utils::matched_keywords(
            &record.message,
            &settings.bug_keywords,
            settings.keyword_match == KeywordMatchMode::Substring,
        );
        if matched_terms.is_empty() {
            continue;
        }
        let paths = scoped_paths(&record.paths, scope);
        if paths.is_empty() {
            continue;
        }
        for path in &paths {
            *counts.entry(path.clone()).or_insert(0) += 1;
        }
        commits.push(record.evidence(paths, matched_terms));
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
        keyword_match: settings.keyword_match,
        paths,
        overlap_paths,
        commits,
        caveats,
    }
}

fn analyze_activity(records: &[CommitRecord], scope: &str) -> ActivityReport {
    let mut months = BTreeMap::new();
    for record in records.iter().filter(|record| record.affects_scope(scope)) {
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
        .filter(|record| !record.is_merge && utils::in_window(record.committer_seconds, now, settings.window_days))
        .filter_map(|record| {
            let matched_terms = utils::matched_keywords(
                &record.message,
                &settings.firefighting_keywords,
                settings.keyword_match == KeywordMatchMode::Substring,
            );
            if matched_terms.is_empty() {
                return None;
            }
            let paths = scoped_paths(&record.paths, scope);
            (!paths.is_empty()).then(|| record.evidence(paths, matched_terms))
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
        keyword_match: settings.keyword_match,
        commits,
        caveats,
    }
}

fn collect_commits(
    repository: &gix::Repository, head: gix::Id<'_>, scope: &str, operation: Option<HistoryOperation>,
    limits: &ReportLimits,
) -> Result<CommitScan> {
    let needs_paths = scope != "."
        || operation.is_none_or(|operation| {
            matches!(
                operation,
                HistoryOperation::Churn | HistoryOperation::Bugs | HistoryOperation::Firefighting
            )
        });
    let needs_message =
        operation.is_none_or(|operation| matches!(operation, HistoryOperation::Bugs | HistoryOperation::Firefighting));
    let walk = repository
        .rev_walk([head])
        .sorting(Sorting::ByCommitTime(CommitTimeOrder::NewestFirst))
        .all()
        .map_err(|error| HistoryError::analysis("walking revisions", error))?;
    let mut records = Vec::new();
    let mut commits_seen = 0usize;
    let mut non_merge_commits_seen = 0usize;
    let mut truncated = false;
    let mut elapsed_limited = false;
    let mut missing_objects = 0usize;
    let scan_started = Instant::now();
    for info in walk {
        if scan_started.elapsed().as_millis() >= u128::from(limits.max_elapsed_ms) {
            truncated = true;
            elapsed_limited = true;
            break;
        }
        let info = match info {
            Ok(info) => info,
            Err(error) if is_missing_object_error(&error) => {
                missing_objects = missing_objects.saturating_add(1);
                truncated = true;
                continue;
            }
            Err(error) => return Err(HistoryError::analysis("walking revisions", error)),
        };
        let id = info.id;
        let commit = match info.object() {
            Ok(commit) => commit,
            Err(error) if is_missing_object_error(&error) => {
                missing_objects = missing_objects.saturating_add(1);
                truncated = true;
                continue;
            }
            Err(error) => return Err(HistoryError::analysis("reading a commit object", error)),
        };
        let author = match commit.author() {
            Ok(author) => author,
            Err(error) if is_missing_object_error(&error) => {
                missing_objects = missing_objects.saturating_add(1);
                truncated = true;
                continue;
            }
            Err(error) => return Err(HistoryError::analysis("decoding a commit author", error)),
        };
        let author_time = match author.time() {
            Ok(time) => time,
            Err(error) if is_missing_object_error(&error) => {
                missing_objects = missing_objects.saturating_add(1);
                truncated = true;
                continue;
            }
            Err(error) => return Err(HistoryError::analysis("decoding an author timestamp", error)),
        };
        let committer = match commit.committer() {
            Ok(committer) => committer,
            Err(error) if is_missing_object_error(&error) => {
                missing_objects = missing_objects.saturating_add(1);
                truncated = true;
                continue;
            }
            Err(error) => return Err(HistoryError::analysis("decoding a commit committer", error)),
        };
        let committer_time = match committer.time() {
            Ok(time) => time,
            Err(error) if is_missing_object_error(&error) => {
                missing_objects = missing_objects.saturating_add(1);
                truncated = true;
                continue;
            }
            Err(error) => return Err(HistoryError::analysis("decoding a committer timestamp", error)),
        };
        let parents: Vec<_> = commit.parent_ids().take(2).collect();
        let is_merge = parents.len() > 1;
        let changed = if needs_paths && !is_merge {
            match changed_paths(repository, &commit, parents.first().copied()) {
                Ok(changed) => changed,
                Err(error) if is_missing_object_error(&error) => {
                    missing_objects = missing_objects.saturating_add(1);
                    truncated = true;
                    continue;
                }
                Err(error) => return Err(error),
            }
        } else {
            ChangedPaths { paths: Vec::new(), truncated: false }
        };
        let ChangedPaths { paths, truncated: changed_truncated } = changed;
        if scope != "." && scoped_paths(&paths, scope).is_empty() {
            continue;
        }
        commits_seen = commits_seen.saturating_add(1);
        if !is_merge {
            non_merge_commits_seen = non_merge_commits_seen.saturating_add(1);
        }
        if records.len() >= limits.max_commits {
            truncated = true;
            break;
        }
        truncated |= changed_truncated;
        let (subject, message) = if needs_message {
            let message = match commit.message_raw() {
                Ok(message) => message.to_str_lossy().into_owned(),
                Err(error) if is_missing_object_error(&error) => {
                    missing_objects = missing_objects.saturating_add(1);
                    truncated = true;
                    continue;
                }
                Err(error) => return Err(HistoryError::analysis("decoding a commit message", error)),
            };
            let subject = match commit.message() {
                Ok(message) => message.summary(),
                Err(error) if is_missing_object_error(&error) => {
                    missing_objects = missing_objects.saturating_add(1);
                    truncated = true;
                    continue;
                }
                Err(error) => return Err(HistoryError::analysis("decoding a commit message", error)),
            };
            (String::from_utf8_lossy(subject.as_ref()).trim().to_owned(), message)
        } else {
            (String::new(), String::new())
        };

        let author_name = author.name.to_str_lossy().trim().to_owned();
        let raw_author_email = author.email.to_str_lossy().trim().to_owned();
        let author_email = raw_author_email.to_lowercase();
        records.push(CommitRecord {
            id: id.to_string(),
            subject,
            message,
            raw_author_name: author_name.clone(),
            raw_author_email,
            author_name: if author_name.is_empty() { "Unknown".to_owned() } else { author_name },
            author_email,
            author_seconds: author_time.seconds,
            committer_seconds: committer_time.seconds,
            is_merge,
            paths,
        });
    }
    Ok(CommitScan { records, commits_seen, non_merge_commits_seen, truncated, elapsed_limited, missing_objects })
}

fn is_missing_object_error(error: &impl std::fmt::Display) -> bool {
    let message = error.to_string().to_lowercase();
    message.contains("missing")
        || message.contains("not found")
        || message.contains("could not be found")
        || message.contains("could not find")
        || message.contains("promisor")
}

fn changed_paths(
    repository: &gix::Repository, commit: &gix::Commit<'_>, parent: Option<gix::Id<'_>>,
) -> Result<ChangedPaths> {
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
    let mut pairs = vec![(previous_tree.id, current_tree.id, String::new())];
    let mut nodes_seen = 0usize;
    let mut truncated = false;
    while let Some((previous_id, current_id, prefix)) = pairs.pop() {
        nodes_seen = nodes_seen.saturating_add(1);
        if nodes_seen > MAX_TREE_NODES_PER_COMMIT || paths.len() >= MAX_CHANGED_PATHS_PER_COMMIT {
            truncated = true;
            break;
        }
        if previous_id == current_id {
            continue;
        }
        let previous_tree = repository
            .find_tree(previous_id)
            .map_err(|error| HistoryError::analysis("reading a previous directory tree", error))?;
        let current_tree = repository
            .find_tree(current_id)
            .map_err(|error| HistoryError::analysis("reading a current directory tree", error))?;
        let (previous_entries, previous_entries_truncated) = tree_entries(&previous_tree)?;
        let (current_entries, current_entries_truncated) = tree_entries(&current_tree)?;
        truncated |= previous_entries_truncated || current_entries_truncated;
        let names: BTreeSet<_> = previous_entries.keys().chain(current_entries.keys()).cloned().collect();
        for name in names.into_iter().rev() {
            let path = if prefix.is_empty() { name.clone() } else { format!("{prefix}/{name}") };
            match (previous_entries.get(&name), current_entries.get(&name)) {
                (Some((previous_mode, previous_id)), Some((current_mode, current_id)))
                    if previous_mode.is_tree() && current_mode.is_tree() =>
                {
                    if previous_id != current_id {
                        pairs.push((previous_id.to_owned(), current_id.to_owned(), path));
                    }
                }
                (Some((previous_mode, previous_id)), Some((current_mode, current_id)))
                    if previous_mode == current_mode && previous_id == current_id => {}
                (Some((previous_mode, previous_id)), Some((current_mode, current_id))) => {
                    truncated |= collect_changed_entry_iterative(
                        repository,
                        *previous_mode,
                        *previous_id,
                        &path,
                        &mut paths,
                        &mut nodes_seen,
                    )?;
                    truncated |= collect_changed_entry_iterative(
                        repository,
                        *current_mode,
                        *current_id,
                        &path,
                        &mut paths,
                        &mut nodes_seen,
                    )?;
                }
                (Some((mode, id)), None) | (None, Some((mode, id))) => {
                    truncated |=
                        collect_changed_entry_iterative(repository, *mode, *id, &path, &mut paths, &mut nodes_seen)?;
                }
                (None, None) => continue,
            }
            if truncated {
                break;
            }
        }
        if truncated {
            break;
        }
    }
    Ok(ChangedPaths { paths: paths.into_iter().collect(), truncated })
}

fn collect_changed_entry_iterative(
    repository: &gix::Repository, mode: gix::objs::tree::EntryMode, id: gix::ObjectId, path: &str,
    changed: &mut BTreeSet<String>, nodes_seen: &mut usize,
) -> Result<bool> {
    let mut stack = vec![(mode, id, path.to_owned())];
    while let Some((mode, id, path)) = stack.pop() {
        *nodes_seen = nodes_seen.saturating_add(1);
        if *nodes_seen > MAX_TREE_NODES_PER_COMMIT || changed.len() >= MAX_CHANGED_PATHS_PER_COMMIT {
            return Ok(true);
        }
        if mode.is_tree() {
            let tree = repository
                .find_tree(id)
                .map_err(|error| HistoryError::analysis("reading a changed directory tree", error))?;
            let (entries, entries_truncated) = tree_entries(&tree)?;
            for (name, (mode, id)) in entries.into_iter().rev() {
                stack.push((mode, id, format!("{path}/{name}")));
            }
            if entries_truncated {
                return Ok(true);
            }
        } else {
            changed.insert(path);
        }
    }
    Ok(false)
}

fn tree_entries(tree: &gix::Tree<'_>) -> Result<(TreeEntryMap, bool)> {
    let decoded = gix::objs::TreeRef::from_bytes(&tree.data, tree.id.kind())
        .map_err(|error| HistoryError::analysis("decoding a directory tree", error))?;
    let mut entries = BTreeMap::new();
    let mut truncated = false;
    for entry in decoded.entries {
        if entries.len() >= MAX_TREE_ENTRIES_PER_DIRECTORY {
            truncated = true;
            break;
        }
        let name = security::validate_component(entry.filename.as_bytes())
            .map_err(|error| HistoryError::safety("decoding a Git tree path", error))?;
        if entries.insert(name, (entry.mode, entry.oid.to_owned())).is_some() {
            return Err(HistoryError::safety(
                "decoding a Git tree path",
                security::PathSafetyError { kind: security::PathSafetyKind::Collision },
            ));
        }
    }
    Ok((entries, truncated))
}

fn committed_mailmap(repository: &gix::Repository, head: gix::Id<'_>) -> Result<Option<gix::mailmap::Snapshot>> {
    let tree = repository
        .find_commit(head)
        .map_err(|error| HistoryError::analysis("reading HEAD for .mailmap", error))?
        .tree()
        .map_err(|error| HistoryError::analysis("reading the HEAD tree for .mailmap", error))?;
    let Some(entry) = tree.find_entry(".mailmap") else {
        return Ok(None);
    };
    if !entry.mode().is_blob() {
        return Ok(None);
    }
    let object = entry
        .object()
        .map_err(|error| HistoryError::analysis("reading the committed .mailmap", error))?;
    Ok(Some(gix::mailmap::Snapshot::from_bytes(&object.data)))
}

fn apply_mailmap(records: &mut [CommitRecord], mailmap: Option<&gix::mailmap::Snapshot>) {
    let Some(mailmap) = mailmap else {
        return;
    };
    for record in records {
        let signature = gix::actor::SignatureRef {
            name: record.raw_author_name.as_bytes().into(),
            email: record.raw_author_email.as_bytes().into(),
            time: "0 +0000",
        };
        let resolved = mailmap.resolve_cow(signature);
        let name = resolved.name.to_str_lossy().trim().to_owned();
        let email = resolved.email.to_str_lossy().trim().to_lowercase();
        record.author_name = if name.is_empty() { "Unknown".to_owned() } else { name };
        record.author_email = email;
    }
}

fn identity_mappings(records: &[&CommitRecord], include_emails: bool) -> Vec<IdentityMapping> {
    let mappings = records
        .iter()
        .filter(|record| {
            record.raw_author_name != record.author_name
                || !record.raw_author_email.eq_ignore_ascii_case(&record.author_email)
        })
        .map(|record| IdentityMapping {
            raw_name: if record.raw_author_name.is_empty() {
                "Unknown".to_owned()
            } else {
                record.raw_author_name.clone()
            },
            raw_email: include_emails
                .then(|| record.raw_author_email.clone())
                .filter(|email| !email.is_empty()),
            canonical_name: record.author_name.clone(),
            canonical_email: include_emails
                .then(|| record.author_email.clone())
                .filter(|email| !email.is_empty()),
        })
        .collect::<BTreeSet<_>>();
    mappings.into_iter().collect()
}

fn add_head_sizes(
    repository: &gix::Repository, head: gix::Id<'_>, paths: &mut [PathCount], limits: &ReportLimits,
) -> Result<()> {
    let tree = repository
        .find_commit(head)
        .map_err(|error| HistoryError::analysis("reading HEAD for normalized churn", error))?
        .tree()
        .map_err(|error| HistoryError::analysis("reading the HEAD tree for normalized churn", error))?;
    let mut total_bytes = 0usize;
    for path in paths.iter_mut().take(limits.max_history_evidence) {
        let entry = tree
            .lookup_entry(path.path.split('/'))
            .map_err(|error| HistoryError::analysis("looking up a churn path in HEAD", error))?;
        let Some(entry) = entry.filter(|entry| entry.mode().is_blob()) else {
            path.size_status = Some("missing_at_head".to_owned());
            continue;
        };
        let size = entry
            .id()
            .header()
            .map_err(|error| HistoryError::analysis("reading a churn path blob header from HEAD", error))?
            .size();
        path.size_bytes = Some(size);
        if size == 0 {
            path.size_status = Some("empty".to_owned());
            continue;
        }
        let Ok(size_usize) = usize::try_from(size) else {
            path.size_status = Some("oversized".to_owned());
            continue;
        };
        if size_usize > limits.max_file_bytes {
            path.size_status = Some("oversized".to_owned());
            continue;
        }
        if total_bytes.saturating_add(size_usize) > limits.max_total_bytes {
            path.size_status = Some("resource_limit".to_owned());
            continue;
        }
        let object = entry
            .object()
            .map_err(|error| HistoryError::analysis("reading a churn path blob from HEAD", error))?;
        total_bytes = total_bytes.saturating_add(size_usize);
        if object.data.contains(&0) {
            path.size_status = Some("binary".to_owned());
        } else {
            path.size_status =
                Some(if looks_generated(&path.path, &object.data) { "generated" } else { "text" }.to_owned());
            path.commits_per_kib_milli = Some(
                (path.commits as u64)
                    .saturating_mul(1_024_000)
                    .checked_div(size)
                    .unwrap_or(0),
            );
        }
    }
    Ok(())
}

fn looks_generated(path: &str, data: &[u8]) -> bool {
    let path = path.to_ascii_lowercase();
    path.contains("/generated/")
        || path.contains("/gen/")
        || path.ends_with(".min.js")
        || path.ends_with(".min.css")
        || data.get(..data.len().min(512)).is_some_and(|prefix| {
            String::from_utf8_lossy(prefix)
                .to_ascii_lowercase()
                .contains("generated")
        })
}

fn contributor_counts(records: Vec<&CommitRecord>, include_emails: bool) -> Vec<ContributorCount> {
    let total = records.len();
    let mut counts = BTreeMap::<String, (String, String, usize)>::new();
    for record in records {
        let key = if record.author_email.is_empty() {
            record.author_name.to_lowercase()
        } else {
            record.author_email.to_lowercase()
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
            email: include_emails.then_some(email).filter(|email| !email.is_empty()),
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

fn absolute_path(path: &Path) -> Result<PathBuf> {
    security::absolute_input_path(path).map_err(|error| HistoryError::analysis("reading the current directory", error))
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
        .map(|(path, commits)| PathCount {
            path,
            commits,
            size_bytes: None,
            commits_per_kib_milli: None,
            size_status: None,
        })
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
