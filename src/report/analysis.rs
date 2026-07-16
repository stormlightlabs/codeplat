use super::*;

#[derive(Clone, Debug)]
struct ReadingCandidate {
    purpose: ReadingPurpose,
    path: String,
    project_root: Option<String>,
    evidence_kinds: BTreeSet<ReadingEvidenceKind>,
    reasons: BTreeSet<String>,
    limitations: BTreeSet<String>,
    score: u64,
    confidence: ConfidenceTier,
}

struct ReadingCandidateInput {
    purpose: ReadingPurpose,
    path: String,
    project_root: Option<String>,
    evidence_kinds: BTreeSet<ReadingEvidenceKind>,
    score: u64,
    confidence: ConfidenceTier,
    reason: String,
    limitations: Vec<String>,
}

pub fn language_provenance(map: Option<&MapReport>) -> BTreeMap<String, LanguageProvenance> {
    let encountered = map.map(|report| &report.query_packs);
    map::language_capabilities()
        .into_iter()
        .filter(|capability| encountered.is_none_or(|packs| packs.contains_key(capability.language.label())))
        .map(|capability| {
            (
                capability.language.label().to_owned(),
                LanguageProvenance {
                    grammar: capability.grammar.to_owned(),
                    grammar_version: capability.grammar_version.to_owned(),
                    query_pack: capability.query_pack.to_owned(),
                    query_pack_version: capability.query_pack_version.to_owned(),
                },
            )
        })
        .collect()
}

pub fn report_quality(history: Option<&HistoryReport>, map: Option<&MapReport>) -> ReportQuality {
    let stale = map.is_some_and(|report| report.cache.status == CacheStatus::Stale || !report.cache.stale.is_empty());
    let map_truncated = map.is_some_and(|report| {
        [
            report.collections.files.truncated,
            report.collections.symbols.truncated,
            report.collections.omissions.truncated,
            report.collections.findings.truncated,
            report.collections.edges.truncated,
            report.collections.ranking.truncated,
            report.collections.snippets.truncated,
            report.collections.landmarks.truncated,
            report.collections.project_roots.truncated,
        ]
        .into_iter()
        .any(|value| value)
    });
    let history_truncated = history.is_some_and(|report| {
        [
            report.collections.commits.truncated,
            report.collections.churn_paths.truncated,
            report.collections.contributor_identity_mappings.truncated,
            report.collections.contributors_overall.truncated,
            report.collections.contributors_recent.truncated,
            report.collections.bug_paths.truncated,
            report.collections.bug_overlap_paths.truncated,
            report.collections.bug_commits.truncated,
            report.collections.activity_months.truncated,
            report.collections.firefighting_commits.truncated,
        ]
        .into_iter()
        .any(|value| value)
    });
    let incomplete =
        history.is_some_and(|report| report.provenance.completeness.status != HistoryCompletenessStatus::Complete);
    let unsupported = map.is_some_and(|report| report.availability.unsupported_paths > 0);
    let partial = map.is_some_and(|report| report.availability.partial_files > 0);
    let mut strict_issues = Vec::new();
    if stale {
        strict_issues.push(StrictIssue::Stale);
    }
    if map_truncated || history_truncated {
        strict_issues.push(StrictIssue::Truncated);
    }
    if incomplete {
        strict_issues.push(StrictIssue::Incomplete);
    }
    if unsupported {
        strict_issues.push(StrictIssue::Unsupported);
    }
    if partial {
        strict_issues.push(StrictIssue::Partial);
    }
    ReportQuality {
        stale,
        truncated: map_truncated || history_truncated,
        incomplete,
        unsupported,
        partial,
        strict_issues,
    }
}

pub fn build_reading_plan(history: &HistoryReport, map: &MapReport) -> ReadingPlan {
    let mut candidates = BTreeMap::<(ReadingPurpose, String), ReadingCandidate>::new();
    let fallback_evidence;
    let evidence = if map.reading_evidence.sources.is_empty() {
        fallback_evidence = ReadingPlanEvidence {
            sources: map
                .files
                .iter()
                .map(|file| ReadingSourceEvidence { path: file.path.clone(), limitations: file.limitations.clone() })
                .collect(),
            ranking: map.ranking.clone(),
            graph: map
                .edges
                .iter()
                .map(|edge| ReadingGraphEvidence { source: edge.source.clone(), target: edge.target.clone() })
                .collect(),
            omissions: map.omissions.clone(),
            landmarks: map.landmarks.clone(),
            project_roots: map.project_roots.clone(),
        };
        &fallback_evidence
    } else {
        &map.reading_evidence
    };
    let source_paths = evidence
        .sources
        .iter()
        .map(|file| (file.path.as_str(), file))
        .collect::<BTreeMap<_, _>>();
    let rank_by_path = evidence
        .ranking
        .iter()
        .map(|rank| (rank.path.as_str(), rank))
        .collect::<BTreeMap<_, _>>();
    let mut graph_links = BTreeMap::<String, usize>::new();
    for edge in &evidence.graph {
        *graph_links.entry(edge.source.clone()).or_default() += 1;
        *graph_links.entry(edge.target.clone()).or_default() += 1;
    }

    for file in &evidence.sources {
        let root = crate::landmarks::project_root_for_path(&file.path, &evidence.project_roots);
        let rank = rank_by_path.get(file.path.as_str()).copied();
        let links = graph_links.get(&file.path).copied().unwrap_or_default();
        let rank_score = rank.map_or(0, |rank| rank.score);
        let mut evidence = [ReadingEvidenceKind::SourceMap].into_iter().collect::<BTreeSet<_>>();
        if links > 0 {
            evidence.insert(ReadingEvidenceKind::Graph);
        }
        if root.is_some() {
            evidence.insert(ReadingEvidenceKind::ProjectTopology);
        }
        add_reading_candidate(
            &mut candidates,
            ReadingCandidateInput {
                purpose: ReadingPurpose::Architecture,
                path: file.path.clone(),
                project_root: root.clone(),
                evidence_kinds: evidence,
                score: rank_score,
                confidence: if links > 0 { ConfidenceTier::High } else { ConfidenceTier::Medium },
                reason: if links > 0 {
                    format!(
                        "qualified lexical edges connect this ranked source to {links} other file-level relationship(s)"
                    )
                } else {
                    "the bounded source-map ranking retained this path as structural context".to_owned()
                },
                limitations: file.limitations.clone(),
            },
        );

        if let Some(rank) = rank
            && rank.focus_matches > 0
        {
            add_reading_candidate(
                &mut candidates,
                ReadingCandidateInput {
                    purpose: ReadingPurpose::Architecture,
                    path: file.path.clone(),
                    project_root: root.clone(),
                    evidence_kinds: [ReadingEvidenceKind::Focus].into_iter().collect(),
                    score: 2_000_000_000u64.saturating_add(rank.focus_matches as u64 * 1_000_000),
                    confidence: ConfidenceTier::High,
                    reason: format!("an explicit focus matched this path {} time(s)", rank.focus_matches),
                    limitations: Vec::new(),
                },
            );
        }
    }

    for landmark in &evidence.landmarks {
        let is_file_landmark = source_paths.contains_key(landmark.path.as_str())
            || evidence.omissions.iter().any(|omission| omission.path == landmark.path);
        if !is_file_landmark {
            continue;
        }
        let (purpose, confidence, score) = match landmark.kind {
            LandmarkKind::AgentInstructions
            | LandmarkKind::ContributorInstructions
            | LandmarkKind::Readme
            | LandmarkKind::Manifest => (
                ReadingPurpose::StartHere,
                ConfidenceTier::High,
                3_000_000_000u64.saturating_add(landmark.priority as u64 * 1_000_000),
            ),
            LandmarkKind::BuildEntryPoint | LandmarkKind::TaskEntryPoint => (
                ReadingPurpose::Runtime,
                ConfidenceTier::High,
                2_500_000_000u64.saturating_add(landmark.priority as u64 * 1_000_000),
            ),
            LandmarkKind::Lockfile | LandmarkKind::Ci | LandmarkKind::Ownership | LandmarkKind::License => (
                ReadingPurpose::SupportingContext,
                ConfidenceTier::Medium,
                500_000_000u64.saturating_add(landmark.priority as u64 * 1_000_000),
            ),
            LandmarkKind::TestRoot => continue,
            LandmarkKind::WorkspaceRoot
            | LandmarkKind::PackageRoot
            | LandmarkKind::Submodule
            | LandmarkKind::NestedRepository
            | LandmarkKind::Unknown => continue,
        };
        let mut evidence = [ReadingEvidenceKind::Landmark].into_iter().collect::<BTreeSet<_>>();
        if landmark.project_root.is_some() {
            evidence.insert(ReadingEvidenceKind::ProjectTopology);
        }
        add_reading_candidate(
            &mut candidates,
            ReadingCandidateInput {
                purpose,
                path: landmark.path.clone(),
                project_root: landmark.project_root.clone(),
                evidence_kinds: evidence,
                score: score.saturating_add(landmark.focus_matches as u64 * 1_000_000),
                confidence,
                reason: format!("recognized {}: {}", landmark.kind.label(), landmark.reason),
                limitations: Vec::new(),
            },
        );
        if landmark.focus_matches > 0 {
            add_reading_candidate(
                &mut candidates,
                ReadingCandidateInput {
                    purpose,
                    path: landmark.path.clone(),
                    project_root: landmark.project_root.clone(),
                    evidence_kinds: [ReadingEvidenceKind::Focus].into_iter().collect(),
                    score: 2_000_000_000u64.saturating_add(landmark.focus_matches as u64 * 1_000_000),
                    confidence: ConfidenceTier::High,
                    reason: format!(
                        "an explicit focus matched this landmark {} time(s)",
                        landmark.focus_matches
                    ),
                    limitations: Vec::new(),
                },
            );
        }
    }

    for landmark in evidence
        .landmarks
        .iter()
        .filter(|landmark| landmark.kind == LandmarkKind::TestRoot)
    {
        let matching_files = source_paths
            .values()
            .filter(|file| file.path.starts_with(&format!("{}/", landmark.path)))
            .collect::<Vec<_>>();
        if !matching_files.is_empty() {
            for file in matching_files {
                let rank_score = rank_by_path.get(file.path.as_str()).map_or(0, |rank| rank.score);
                let mut evidence = [ReadingEvidenceKind::Landmark, ReadingEvidenceKind::SourceMap]
                    .into_iter()
                    .collect::<BTreeSet<_>>();
                if landmark.project_root.is_some() {
                    evidence.insert(ReadingEvidenceKind::ProjectTopology);
                }
                add_reading_candidate(
                    &mut candidates,
                    ReadingCandidateInput {
                        purpose: ReadingPurpose::Tests,
                        path: file.path.clone(),
                        project_root: landmark.project_root.clone(),
                        evidence_kinds: evidence,
                        score: 1_500_000_000u64.saturating_add(rank_score),
                        confidence: ConfidenceTier::High,
                        reason: format!("the path is inside the recognized test root {}", landmark.path),
                        limitations: file.limitations.clone(),
                    },
                );
            }
        }
    }

    for root in &evidence.project_roots {
        for manifest in &root.manifests {
            if evidence.landmarks.iter().any(|landmark| landmark.path == *manifest) {
                continue;
            }
            add_reading_candidate(
                &mut candidates,
                ReadingCandidateInput {
                    purpose: ReadingPurpose::StartHere,
                    path: manifest.clone(),
                    project_root: Some(root.path.clone()),
                    evidence_kinds: [ReadingEvidenceKind::Landmark, ReadingEvidenceKind::ProjectTopology]
                        .into_iter()
                        .collect(),
                    score: 3_000_000_000,
                    confidence: ConfidenceTier::High,
                    reason: format!("project root manifest for the {} root", root.kind.label()),
                    limitations: Vec::new(),
                },
            );
        }
    }

    if let Some(churn) = &history.churn {
        for path in &churn.paths {
            add_history_candidate(
                &mut candidates,
                &source_paths,
                path,
                format!(
                    "bounded history overlap found {} recent commit(s) touching this path",
                    path.commits
                ),
                path.commits,
                &evidence.project_roots,
            );
        }
    }
    if let Some(bugs) = &history.bugs {
        for path in &bugs.overlap_paths {
            add_history_candidate(
                &mut candidates,
                &source_paths,
                path,
                format!(
                    "bug-related history overlaps this path across {} commit(s)",
                    path.commits
                ),
                path.commits,
                &evidence.project_roots,
            );
        }
    }
    if let Some(firefighting) = &history.firefighting {
        let mut paths = BTreeMap::<String, usize>::new();
        for commit in &firefighting.commits {
            for path in &commit.paths {
                *paths.entry(path.clone()).or_default() += 1;
            }
        }
        for (path, commits) in paths {
            let Some(file) = source_paths.get(path.as_str()) else {
                continue;
            };
            let root = crate::landmarks::project_root_for_path(&file.path, &evidence.project_roots);
            add_reading_candidate(
                &mut candidates,
                ReadingCandidateInput {
                    purpose: ReadingPurpose::SupportingContext,
                    path,
                    project_root: root,
                    evidence_kinds: [ReadingEvidenceKind::HistoryOverlap].into_iter().collect(),
                    score: 400_000_000u64.saturating_add(commits as u64 * 100_000),
                    confidence: ConfidenceTier::Medium,
                    reason: format!("firefighting-language commits touched this path {} time(s)", commits),
                    limitations: file.limitations.clone(),
                },
            );
        }
    }

    let mut selected = Vec::new();
    let mut selected_paths = BTreeSet::new();
    for purpose in [
        ReadingPurpose::StartHere,
        ReadingPurpose::Architecture,
        ReadingPurpose::Runtime,
        ReadingPurpose::Tests,
        ReadingPurpose::SupportingContext,
    ] {
        if let Some(candidate) = best_candidate(
            candidates.values().filter(|candidate| candidate.purpose == purpose),
            &selected_paths,
        ) {
            selected_paths.insert(candidate.path.clone());
            selected.push(candidate);
        }
    }

    for root in &evidence.project_roots {
        if selected.len() >= 10 {
            break;
        }
        if let Some(candidate) = best_candidate(
            candidates
                .values()
                .filter(|candidate| candidate.project_root.as_deref() == Some(root.path.as_str())),
            &selected_paths,
        ) {
            selected_paths.insert(candidate.path.clone());
            selected.push(candidate);
        }
    }

    let mut remaining = candidates.values().collect::<Vec<_>>();
    remaining.sort_by(|left, right| candidate_order(left, right));
    for candidate in remaining {
        if selected.len() >= 10 || selected_paths.contains(&candidate.path) {
            continue;
        }
        selected_paths.insert(candidate.path.clone());
        selected.push(candidate.clone());
    }
    selected.sort_by(|left, right| {
        left.purpose
            .order()
            .cmp(&right.purpose.order())
            .then_with(|| right.score.cmp(&left.score))
            .then_with(|| left.path.cmp(&right.path))
    });

    let recommendations = selected
        .into_iter()
        .enumerate()
        .map(|(index, candidate)| ReadingRecommendation {
            ordinal: index + 1,
            purpose: candidate.purpose,
            path: candidate.path,
            project_root: candidate.project_root,
            reason: candidate.reasons.into_iter().take(2).collect::<Vec<_>>().join("; "),
            evidence_kinds: candidate.evidence_kinds.into_iter().collect(),
            confidence: candidate.confidence,
            limitations: candidate.limitations.into_iter().collect(),
        })
        .collect::<Vec<_>>();

    let mut omitted_project_roots = Vec::new();
    for root in &evidence.project_roots {
        if candidates
            .values()
            .any(|candidate| candidate.project_root.as_deref() == Some(root.path.as_str()))
            && !recommendations
                .iter()
                .any(|recommendation| recommendation.project_root.as_deref() == Some(root.path.as_str()))
        {
            omitted_project_roots.push(ReadingPlanRootOmission {
                project_root: root.path.clone(),
                reason: if recommendations.len() >= 10 {
                    "the bounded ten-path plan prioritized stronger evidence or explicit focus in other roots"
                        .to_owned()
                } else {
                    "eligible paths were unavailable after scope, exclusion, and safety limits".to_owned()
                },
            });
        }
    }

    let candidate_path_count = candidates
        .values()
        .map(|candidate| candidate.path.as_str())
        .collect::<BTreeSet<_>>()
        .len();
    let shortfall = (recommendations.len() < 5).then(|| ReadingPlanShortfall {
        target_minimum: 5,
        returned: recommendations.len(),
        reason: if candidate_path_count < 5 {
            format!(
                "only {} unique paths had retained landmark, source-map, test, runtime, or bounded history evidence",
                candidate_path_count
            )
        } else {
            "the selected scope and safety limits left fewer than five usable paths".to_owned()
        },
    });

    let mut limitations = Vec::new();
    if history.churn.is_none() && history.bugs.is_none() && history.firefighting.is_none() {
        limitations.push("No path-level history signal was available for supporting context.".to_owned());
    }
    if !omitted_project_roots.is_empty() {
        limitations.push(
            "Project-root omissions are reported explicitly; the plan is not padded with low-confidence paths."
                .to_owned(),
        );
    }
    ReadingPlan { recommendations, omitted_project_roots, shortfall, limitations }
}

pub fn briefing_summary(history: &HistoryReport, map: &MapReport) -> String {
    format!(
        "Analyzed {} reachable commits ({} non-merge) and {} source files; ranked {} files within a {}-token source-map budget, with {} paths omitted in the selected scope.",
        history.commits_seen,
        history.non_merge_commits_seen,
        map.inventory.analyzed,
        map.ranking.len(),
        map.selection.token_budget,
        map.inventory.omitted,
    )
}

pub fn explain_report(target: &str, map: &MapReport, history: &HistoryReport) -> ExplainReport {
    let target = target.trim().to_owned();
    let normalized_target = target.trim_start_matches("./");
    let exact_path = map.files.iter().any(|file| file.path == normalized_target)
        || map.omissions.iter().any(|omission| omission.path == normalized_target)
        || (target.contains('/') && std::path::Path::new(&target).extension().is_some());
    let mut matched_paths = std::collections::BTreeSet::new();
    let mut matched_symbols = Vec::new();
    if exact_path {
        matched_paths.insert(normalized_target.to_owned());
    }
    for file in &map.files {
        for symbol in &file.symbols {
            let qualified = if symbol.scope.is_empty() {
                symbol.name.clone()
            } else {
                format!("{}::{}", symbol.scope.join("::"), symbol.name)
            };
            if !exact_path && (symbol.name == target || qualified == target) {
                matched_paths.insert(file.path.clone());
                if matched_symbols.len() < 128 {
                    matched_symbols.push(ExplainSymbolMatch { path: file.path.clone(), symbol: symbol.clone() });
                }
            }
        }
    }
    let target_kind = if exact_path {
        ExplainTargetKind::Path
    } else if !matched_symbols.is_empty() {
        ExplainTargetKind::Symbol
    } else {
        ExplainTargetKind::Unmatched
    };

    let mut focus_matches = map
        .ranking
        .iter()
        .filter(|rank| matched_paths.contains(&rank.path) && rank.focus_matches > 0)
        .map(|rank| rank.path.clone())
        .collect::<Vec<_>>();
    focus_matches.sort();
    focus_matches.dedup();

    let mut incoming = BTreeMap::<String, usize>::new();
    let mut outgoing = BTreeMap::<String, usize>::new();
    for edge in &map.edges {
        *incoming.entry(edge.target.clone()).or_default() += 1;
        *outgoing.entry(edge.source.clone()).or_default() += 1;
    }
    let ranking = map
        .ranking
        .iter()
        .filter(|rank| matched_paths.contains(&rank.path))
        .map(|rank| ExplainRanking {
            path: rank.path.clone(),
            score: rank.score,
            focus_matches: rank.focus_matches,
            incoming_edges: incoming.get(&rank.path).copied().unwrap_or_default(),
            outgoing_edges: outgoing.get(&rank.path).copied().unwrap_or_default(),
        })
        .collect::<Vec<_>>();

    let graph_edges = map
        .edges
        .iter()
        .filter(|edge| {
            matched_paths.contains(&edge.source) || matched_paths.contains(&edge.target) || edge.symbol == target
        })
        .take(128)
        .cloned()
        .collect::<Vec<_>>();
    let ambiguity = map
        .findings
        .iter()
        .filter(|finding| {
            finding.kind == MapFindingKind::AmbiguousReference
                && (matched_paths.contains(&finding.path) || finding.detail.contains(&target))
        })
        .take(64)
        .cloned()
        .collect::<Vec<_>>();

    let mut history_overlap = Vec::new();
    for paths in [
        history.churn.as_ref().map(|report| report.paths.as_slice()),
        history.bugs.as_ref().map(|report| report.overlap_paths.as_slice()),
    ]
    .into_iter()
    .flatten()
    .flatten()
    {
        if matched_paths.contains(&paths.path)
            && !history_overlap.iter().any(|path: &PathCount| path.path == paths.path)
        {
            history_overlap.push(paths.clone());
        }
    }
    history_overlap.sort_by(|left, right| left.path.cmp(&right.path));

    let omitted_alternatives = map
        .omissions
        .iter()
        .filter(|omission| target_kind == ExplainTargetKind::Unmatched || matched_paths.contains(&omission.path))
        .take(32)
        .cloned()
        .collect::<Vec<_>>();
    let landmark = matched_paths.iter().find_map(|path| landmark_for_path(path));
    let mut limitations = vec![
        "This explanation describes bounded lexical evidence and ranking heuristics; it is not a semantic call graph or access check.".to_owned(),
    ];
    if target_kind == ExplainTargetKind::Unmatched {
        limitations.push(
            "The target did not match an analyzed path or symbol; omitted alternatives are shown when available."
                .to_owned(),
        );
    }
    if map.collections.edges.truncated || map.collections.ranking.truncated {
        limitations.push("Graph and ranking evidence was truncated by the active report profile.".to_owned());
    }
    ExplainReport {
        target,
        target_kind,
        matched_paths: matched_paths.into_iter().collect(),
        matched_symbols,
        focus_matches,
        history_overlap,
        landmark,
        ranking,
        graph_edges,
        ambiguity,
        omitted_alternatives,
        limitations,
    }
}

fn add_reading_candidate(
    candidates: &mut BTreeMap<(ReadingPurpose, String), ReadingCandidate>, input: ReadingCandidateInput,
) {
    let ReadingCandidateInput { purpose, path, project_root, evidence_kinds, score, confidence, reason, limitations } =
        input;
    if path.is_empty() {
        return;
    }
    let key = (purpose, path.clone());
    let candidate = candidates.entry(key).or_insert_with(|| ReadingCandidate {
        purpose,
        path,
        project_root: None,
        evidence_kinds: BTreeSet::new(),
        reasons: BTreeSet::new(),
        limitations: BTreeSet::new(),
        score: 0,
        confidence,
    });
    if candidate.project_root.is_none() {
        candidate.project_root = project_root;
    }
    candidate.evidence_kinds.extend(evidence_kinds);
    if !reason.is_empty() {
        candidate.reasons.insert(reason);
    }
    candidate.limitations.extend(limitations);
    candidate.score = candidate.score.saturating_add(score);
    if confidence_strength(confidence) > confidence_strength(candidate.confidence) {
        candidate.confidence = confidence;
    }
}

fn add_history_candidate(
    candidates: &mut BTreeMap<(ReadingPurpose, String), ReadingCandidate>,
    source_paths: &BTreeMap<&str, &ReadingSourceEvidence>, path: &PathCount, reason: String, commits: usize,
    project_roots: &[ProjectRoot],
) {
    let Some(file) = source_paths.get(path.path.as_str()) else {
        return;
    };
    let root = crate::landmarks::project_root_for_path(&file.path, project_roots);
    add_reading_candidate(
        candidates,
        ReadingCandidateInput {
            purpose: ReadingPurpose::SupportingContext,
            path: file.path.clone(),
            project_root: root,
            evidence_kinds: [ReadingEvidenceKind::HistoryOverlap].into_iter().collect(),
            score: 600_000_000u64.saturating_add(commits as u64 * 100_000),
            confidence: ConfidenceTier::Medium,
            reason,
            limitations: file.limitations.clone(),
        },
    );
}

fn best_candidate<'a>(
    candidates: impl Iterator<Item = &'a ReadingCandidate>, selected_paths: &BTreeSet<String>,
) -> Option<ReadingCandidate> {
    candidates
        .filter(|candidate| !selected_paths.contains(&candidate.path))
        .max_by(|left, right| candidate_order(left, right))
        .cloned()
}

fn candidate_order(left: &ReadingCandidate, right: &ReadingCandidate) -> std::cmp::Ordering {
    left.score
        .cmp(&right.score)
        .then_with(|| confidence_strength(left.confidence).cmp(&confidence_strength(right.confidence)))
        .then_with(|| right.path.cmp(&left.path))
}

fn confidence_strength(confidence: ConfidenceTier) -> u8 {
    match confidence {
        ConfidenceTier::High => 3,
        ConfidenceTier::Medium => 2,
        ConfidenceTier::Low => 1,
    }
}

fn landmark_for_path(path: &str) -> Option<ExplainLandmark> {
    let name = path.rsplit('/').next().unwrap_or(path);
    let (kind, reason) = match name {
        "README" | "README.md" | "README.rst" | "README.txt" => {
            ("readme", "conventional repository orientation document")
        }
        "AGENTS.md" | "CONTRIBUTING.md" | "CLAUDE.md" => ("instructions", "repository instruction file"),
        "Cargo.toml" | "package.json" | "pyproject.toml" | "go.mod" | "pom.xml" => {
            ("manifest", "project manifest or package root")
        }
        _ if name.ends_with(".csproj") => ("manifest", "project manifest or package root"),
        "Cargo.lock" | "package-lock.json" | "pnpm-lock.yaml" | "poetry.lock" => ("lockfile", "dependency lockfile"),
        _ if path.contains("/.github/workflows/") || path.starts_with(".github/workflows/") => {
            ("ci", "continuous-integration workflow")
        }
        _ if path.starts_with("tests/") || path.contains("/tests/") => ("tests", "test root"),
        _ => return None,
    };
    Some(ExplainLandmark { kind: kind.to_owned(), path: path.to_owned(), reason: reason.to_owned() })
}
