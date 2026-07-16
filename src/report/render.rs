use std::collections::BTreeMap;
use std::fmt::Write;

use crate::utils;

pub struct Render;

impl Render {
    fn commits(output: &mut String, commits: &[super::CommitEvidence]) {
        writeln!(output, "#### Evidence commits").expect("writing to a string cannot fail");
        if commits.is_empty() {
            writeln!(output, "No matching commits were found.").expect("writing to a string cannot fail");
        } else {
            for commit in commits {
                let paths =
                    if commit.paths.is_empty() { "no in-scope paths".to_owned() } else { commit.paths.join(", ") };
                writeln!(
                    output,
                    "- `{}` — {} ({}){}",
                    utils::escape_inline_code(&commit.id),
                    utils::sanitize_text(&commit.subject),
                    utils::sanitize_text(&paths),
                    if commit.matched_terms.is_empty() {
                        String::new()
                    } else {
                        format!(" — matched {}", utils::inline_code_list(&commit.matched_terms))
                    }
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

    pub fn briefing_overview(output: &mut String, map: &super::MapReport) {
        writeln!(output).expect("writing to a string cannot fail");
        writeln!(output, "## Repository overview").expect("writing to a string cannot fail");
        writeln!(output).expect("writing to a string cannot fail");
        writeln!(
            output,
            "Repository: `{}`",
            utils::escape_inline_code(&map.repository_root)
        )
        .expect("writing to a string cannot fail");
        writeln!(output, "Worktree: {}", map.worktree.state.label()).expect("writing to a string cannot fail");

        let mut languages = map
            .files
            .iter()
            .map(|file| file.language.display_label())
            .collect::<Vec<_>>();
        languages.sort_unstable();
        languages.dedup();
        writeln!(
            output,
            "Primary supported languages: {}",
            if languages.is_empty() { "none detected".to_owned() } else { languages.join(", ") }
        )
        .expect("writing to a string cannot fail");
        writeln!(
            output,
            "Project roots: {} detected; {} source files analyzed",
            map.project_roots.len(),
            map.inventory.analyzed
        )
        .expect("writing to a string cannot fail");

        let landmarks = map
            .landmarks
            .iter()
            .filter(|landmark| {
                matches!(
                    landmark.kind,
                    super::LandmarkKind::AgentInstructions
                        | super::LandmarkKind::ContributorInstructions
                        | super::LandmarkKind::Readme
                        | super::LandmarkKind::Manifest
                        | super::LandmarkKind::WorkspaceRoot
                        | super::LandmarkKind::PackageRoot
                )
            })
            .take(8)
            .collect::<Vec<_>>();
        if landmarks.is_empty() {
            writeln!(output, "Orientation landmarks: none detected in the selected scope.")
                .expect("writing to a string cannot fail");
        } else {
            writeln!(output, "Orientation landmarks:").expect("writing to a string cannot fail");
            for landmark in landmarks {
                writeln!(
                    output,
                    "- **{}** `{}` — {}",
                    landmark.kind.label(),
                    utils::escape_inline_code(&landmark.path),
                    utils::sanitize_text(&landmark.reason)
                )
                .expect("writing to a string cannot fail");
            }
        }
    }

    pub fn reading_plan_markdown(output: &mut String, plan: &super::ReadingPlan) {
        writeln!(output).expect("writing to a string cannot fail");
        writeln!(output, "## Reading plan").expect("writing to a string cannot fail");
        writeln!(output).expect("writing to a string cannot fail");
        if plan.recommendations.is_empty() {
            writeln!(output, "No evidence-backed paths were selected for the reading plan.")
                .expect("writing to a string cannot fail");
        } else {
            let mut current_purpose = None;
            for recommendation in &plan.recommendations {
                if current_purpose != Some(recommendation.purpose) {
                    if current_purpose.is_some() {
                        writeln!(output).expect("writing to a string cannot fail");
                    }
                    current_purpose = Some(recommendation.purpose);
                    writeln!(output, "### {}", recommendation.purpose.label())
                        .expect("writing to a string cannot fail");
                    writeln!(output).expect("writing to a string cannot fail");
                }
                let root = recommendation
                    .project_root
                    .as_deref()
                    .map(|root| format!(", project root `{}`", utils::escape_inline_code(root)))
                    .unwrap_or_default();
                let evidence = recommendation
                    .evidence_kinds
                    .iter()
                    .map(|kind| kind.label())
                    .collect::<Vec<_>>()
                    .join(", ");
                writeln!(
                    output,
                    "{}. `{}`{} — {} ({} confidence; evidence: {})",
                    recommendation.ordinal,
                    utils::escape_inline_code(&recommendation.path),
                    root,
                    utils::sanitize_text(&recommendation.reason),
                    recommendation.confidence.label(),
                    evidence
                )
                .expect("writing to a string cannot fail");
                for limitation in &recommendation.limitations {
                    writeln!(output, "   - Limitation: {}", utils::sanitize_text(limitation))
                        .expect("writing to a string cannot fail");
                }
            }
        }
        if let Some(shortfall) = &plan.shortfall {
            writeln!(
                output,
                "Short plan: {} of {} minimum recommendations — {}",
                shortfall.returned,
                shortfall.target_minimum,
                utils::sanitize_text(&shortfall.reason)
            )
            .expect("writing to a string cannot fail");
        }
        for omission in &plan.omitted_project_roots {
            writeln!(
                output,
                "Omitted project root `{}` — {}",
                utils::escape_inline_code(&omission.project_root),
                utils::sanitize_text(&omission.reason)
            )
            .expect("writing to a string cannot fail");
        }
        if !plan.limitations.is_empty() {
            writeln!(output, "Plan limitations:").expect("writing to a string cannot fail");
            for limitation in &plan.limitations {
                writeln!(output, "- {}", utils::sanitize_text(limitation)).expect("writing to a string cannot fail");
            }
        }
    }

    pub fn history_markdown(output: &mut String, history: &super::HistoryReport) {
        Render::history_header(output, history);

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
        Render::history_limitations(output, history);
    }

    pub fn history_briefing_markdown(output: &mut String, history: &super::HistoryReport) {
        Render::history_header(output, history);
        Render::section_heading(output, "History observations");
        if history.observations.is_empty() {
            writeln!(
                output,
                "No distinct observations were supported by the available history evidence."
            )
            .expect("writing to a string cannot fail");
        } else {
            for observation in &history.observations {
                Render::history_observation(output, observation);
            }
        }
        writeln!(
            output,
            "Detailed history evidence: use `codeplat history`, a focused history subcommand, `--profile evidence`, or `--json`."
        )
        .expect("writing to a string cannot fail");
        Render::history_limitations(output, history);
    }

    fn history_header(output: &mut String, history: &super::HistoryReport) {
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
            "Bug keywords: {}",
            utils::inline_code_list(&history.settings.bug_keywords)
        )
        .expect("writing to a string cannot fail");
        writeln!(
            output,
            "Firefighting keywords: {}",
            utils::inline_code_list(&history.settings.firefighting_keywords)
        )
        .expect("writing to a string cannot fail");
        writeln!(output, "Keyword matching: {}", history.settings.keyword_match.label())
            .expect("writing to a string cannot fail");
        if history.collections.commits.truncated
            || history.collections.churn_paths.truncated
            || history.collections.contributor_identity_mappings.truncated
            || history.collections.contributors_overall.truncated
            || history.collections.contributors_recent.truncated
            || history.collections.bug_paths.truncated
            || history.collections.bug_overlap_paths.truncated
            || history.collections.bug_commits.truncated
            || history.collections.activity_months.truncated
            || history.collections.firefighting_commits.truncated
        {
            writeln!(
                output,
                "Evidence collections are bounded; JSON contains totals and truncation reasons."
            )
            .expect("writing to a string cannot fail");
        }
    }

    fn history_limitations(output: &mut String, history: &super::HistoryReport) {
        for limitation in &history.limitations {
            writeln!(output, "- Limitation: {}", utils::sanitize_text(limitation))
                .expect("writing to a string cannot fail");
        }
    }

    fn history_observation(output: &mut String, observation: &super::HistoryObservation) {
        match observation {
            super::HistoryObservation::Churn { paths, window_days, caveat } => {
                writeln!(
                    output,
                    "- **Churn:** {} changed over the last {} days. Caveat: {}",
                    Render::path_counts_inline(paths),
                    window_days,
                    utils::sanitize_text(caveat)
                )
                .expect("writing to a string cannot fail");
            }
            super::HistoryObservation::Contributors { contributor, total_commits, window_days, caveat } => {
                let window = window_days.map_or_else(
                    || "across observed history".to_owned(),
                    |days| format!("in the recent {days}-day window"),
                );
                writeln!(
                    output,
                    "- **Contributor concentration:** {} authored {} of {} non-merge commits ({}%) {}. Caveat: {}",
                    utils::sanitize_text(&contributor.name),
                    contributor.commits,
                    total_commits,
                    contributor.share_percent,
                    window,
                    utils::sanitize_text(caveat)
                )
                .expect("writing to a string cannot fail");
            }
            super::HistoryObservation::BugOverlap { paths, bug_commits, window_days, caveat } => {
                writeln!(
                    output,
                    "- **Bug/churn overlap:** {} overlapped across {} matching bug commits in the last {} days. Caveat: {}",
                    Render::path_counts_inline(paths),
                    bug_commits,
                    window_days,
                    utils::sanitize_text(caveat)
                )
                .expect("writing to a string cannot fail");
            }
            super::HistoryObservation::Activity { month, commits, observed_months, observed_commits, caveat } => {
                writeln!(
                    output,
                    "- **Activity:** `{}` was the busiest observed month with {} commits across {} observed commits and {} months. Caveat: {}",
                    utils::escape_inline_code(month),
                    commits,
                    observed_commits,
                    observed_months,
                    utils::sanitize_text(caveat)
                )
                .expect("writing to a string cannot fail");
            }
            super::HistoryObservation::Firefighting { commits, paths, window_days, caveat } => {
                writeln!(
                    output,
                    "- **Firefighting language:** {} matching commits touched {} over the last {} days. Caveat: {}",
                    commits,
                    Render::path_counts_inline(paths),
                    window_days,
                    utils::sanitize_text(caveat)
                )
                .expect("writing to a string cannot fail");
            }
        }
    }

    fn path_counts_inline(paths: &[super::PathCount]) -> String {
        paths
            .iter()
            .map(|path| format!("`{}` ({} commits)", utils::escape_inline_code(&path.path), path.commits))
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub fn map_markdown(output: &mut String, map: &super::MapReport) {
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
            "Inventory: {} tracked ({} modified), {} untracked, {} analyzed, {} omitted, {} classified",
            map.inventory.tracked,
            map.inventory.modified,
            map.inventory.untracked,
            map.inventory.analyzed,
            map.inventory.omitted,
            map.classifications.total
        )
        .expect("writing to a string cannot fail");
        if map.classifications.total > 0 {
            writeln!(
                output,
                "Classifications: {} paths ({} generated, {} vendor, {} minified, {} source maps); {} samples returned{}",
                map.classifications.total,
                map.classifications.generated,
                map.classifications.vendor,
                map.classifications.minified,
                map.classifications.source_map,
                map.classifications.returned,
                if map.classifications.truncated { "; sample truncated" } else { "" }
            )
            .expect("writing to a string cannot fail");
            Render::section_heading(output, "Generated, vendor, and minified paths");
            for sample in &map.classifications.samples {
                let reasons = sample
                    .classifications
                    .iter()
                    .map(|classification| {
                        format!(
                            "{} ({})",
                            classification.kind.label(),
                            utils::sanitize_text(&classification.reason)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                writeln!(
                    output,
                    "- `{}` — {} [{}]",
                    utils::escape_inline_code(&sample.path),
                    if sample.overridden {
                        "included by explicit focus/evidence override"
                    } else {
                        "excluded before parsing"
                    },
                    reasons
                )
                .expect("writing to a string cannot fail");
            }
        }
        if map.collections.files.truncated
            || map.collections.symbols.truncated
            || map.collections.omissions.truncated
            || map.collections.findings.truncated
            || map.collections.edges.truncated
            || map.collections.ranking.truncated
            || map.collections.snippets.truncated
            || map.collections.landmarks.truncated
            || map.collections.project_roots.truncated
        {
            writeln!(
                output,
                "Collections are bounded; JSON contains totals and truncation reasons."
            )
            .expect("writing to a string cannot fail");
        }
        if !map.exclusions.is_empty() {
            writeln!(output, "Exclusions: {}", utils::inline_code_list(&map.exclusions))
                .expect("writing to a string cannot fail");
        }

        if !map.landmarks.is_empty() || !map.project_roots.is_empty() {
            Render::section_heading(output, "Repository landmarks");
            writeln!(
                output,
                "Landmarks: {} returned of {}; project roots: {} returned of {}",
                map.collections.landmarks.returned,
                map.collections.landmarks.total,
                map.collections.project_roots.returned,
                map.collections.project_roots.total
            )
            .expect("writing to a string cannot fail");
            for root in &map.project_roots {
                writeln!(
                    output,
                    "- Project root `{}` — {} — {}",
                    utils::escape_inline_code(&root.path),
                    root.kind.label(),
                    utils::sanitize_text(&root.reason)
                )
                .expect("writing to a string cannot fail");
                if !root.recommended_paths.is_empty() {
                    writeln!(
                        output,
                        "  - Recommended source paths: {}",
                        utils::inline_code_list(&root.recommended_paths)
                    )
                    .expect("writing to a string cannot fail");
                }
            }
            for landmark in &map.landmarks {
                writeln!(
                    output,
                    "- **{}** `{}` — {} [{}{}]",
                    landmark.kind.label(),
                    utils::escape_inline_code(&landmark.path),
                    utils::sanitize_text(&landmark.reason),
                    landmark.worktree_state.label(),
                    landmark.project_root.as_deref().map_or(String::new(), |root| {
                        format!(", project root `{}`", utils::escape_inline_code(root))
                    })
                )
                .expect("writing to a string cannot fail");
            }
        }

        if !map.files.is_empty()
            || map.cache.matched > 0
            || map.cache.unmatched > 0
            || map.cache.unavailable > 0
            || map.cache.hits > 0
            || map.cache.misses > 0
            || !map.cache.refreshed.is_empty()
            || !map.cache.stale.is_empty()
        {
            writeln!(
                output,
                "Cache: {} ({}) — {} matched, {} unmatched, {} unavailable, {} hits, {} misses, {} refreshed, {} stale",
                map.cache.mode.label(),
                map.cache.status.label(),
                map.cache.matched,
                map.cache.unmatched,
                map.cache.unavailable,
                map.cache.hits,
                map.cache.misses,
                map.cache.refreshed.len(),
                map.cache.stale.len()
            )
            .expect("writing to a string cannot fail");
            if !map.files.is_empty() {
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
        }

        let mut files_by_language: BTreeMap<super::SourceLanguage, Vec<&super::SourceFile>> = BTreeMap::new();
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
                    "- `{}` → `{}` via `{}` — {} / {}{}",
                    utils::escape_inline_code(&edge.source),
                    utils::escape_inline_code(&edge.target),
                    utils::escape_inline_code(&edge.symbol),
                    edge.resolution_reason.label(),
                    edge.confidence.label(),
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

    pub fn explain_markdown(output: &mut String, explain: &super::ExplainReport) {
        Render::section_heading(output, "Recommendation explanation");
        writeln!(
            output,
            "Target: `{}` ({:?})",
            utils::escape_inline_code(&explain.target),
            explain.target_kind
        )
        .expect("writing to a string cannot fail");
        if !explain.matched_paths.is_empty() {
            writeln!(
                output,
                "Matched paths: {}",
                utils::inline_code_list(&explain.matched_paths)
            )
            .expect("writing to a string cannot fail");
        }
        if !explain.focus_matches.is_empty() {
            writeln!(
                output,
                "Focus evidence: {}",
                utils::inline_code_list(&explain.focus_matches)
            )
            .expect("writing to a string cannot fail");
        }
        if let Some(landmark) = &explain.landmark {
            writeln!(
                output,
                "Landmark: **{}** `{}` — {}",
                landmark.kind,
                utils::escape_inline_code(&landmark.path),
                landmark.reason
            )
            .expect("writing to a string cannot fail");
        }
        if !explain.ranking.is_empty() {
            writeln!(output, "Ranking evidence:").expect("writing to a string cannot fail");
            for rank in &explain.ranking {
                writeln!(
                    output,
                    "- `{}` — score {}, {} incoming, {} outgoing edges",
                    utils::escape_inline_code(&rank.path),
                    rank.score,
                    rank.incoming_edges,
                    rank.outgoing_edges
                )
                .expect("writing to a string cannot fail");
            }
        }
        if !explain.history_overlap.is_empty() {
            writeln!(output, "History overlap:").expect("writing to a string cannot fail");
            for path in &explain.history_overlap {
                writeln!(
                    output,
                    "- `{}` — {} commits",
                    utils::escape_inline_code(&path.path),
                    path.commits
                )
                .expect("writing to a string cannot fail");
            }
        }
        if !explain.graph_edges.is_empty() {
            writeln!(output, "Graph evidence:").expect("writing to a string cannot fail");
            for edge in &explain.graph_edges {
                writeln!(
                    output,
                    "- `{}` → `{}` via `{}` — {} / {}",
                    utils::escape_inline_code(&edge.source),
                    utils::escape_inline_code(&edge.target),
                    utils::escape_inline_code(&edge.symbol),
                    edge.resolution_reason.label(),
                    edge.confidence.label()
                )
                .expect("writing to a string cannot fail");
            }
        }
        if !explain.ambiguity.is_empty() {
            writeln!(output, "Ambiguity:").expect("writing to a string cannot fail");
            for finding in &explain.ambiguity {
                writeln!(output, "- {}", utils::sanitize_text(&finding.detail))
                    .expect("writing to a string cannot fail");
            }
        }
        if !explain.omitted_alternatives.is_empty() {
            writeln!(output, "Omitted alternatives:").expect("writing to a string cannot fail");
            for omission in &explain.omitted_alternatives {
                writeln!(
                    output,
                    "- `{}` — {}",
                    utils::escape_inline_code(&omission.path),
                    omission.reason.label()
                )
                .expect("writing to a string cannot fail");
            }
        }
        Render::caveats(output, &explain.limitations);
    }

    fn source_files(output: &mut String, files: &[&super::SourceFile]) {
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

    fn format_location(location: &super::SourceLocation) -> String {
        format!(
            "{}:{}-{}:{}",
            location.start.line, location.start.column, location.end.line, location.end.column
        )
    }

    fn churn_markdown(output: &mut String, churn: &super::ChurnReport) {
        Render::section_heading(output, "Churn hotspots");
        writeln!(output, "Window: {} days", churn.window_days).expect("writing to a string cannot fail");
        if churn.paths.is_empty() {
            writeln!(output, "No in-scope non-merge paths changed in this window.")
                .expect("writing to a string cannot fail");
        } else {
            for path in &churn.paths {
                let normalized = path.commits_per_kib_milli.map_or_else(
                    || {
                        format!(
                            "normalization unavailable ({})",
                            path.size_status.as_deref().unwrap_or("unknown")
                        )
                    },
                    |rate| {
                        format!(
                            "{:.3} commits/KiB ({})",
                            rate as f64 / 1_000.0,
                            path.size_status.as_deref().unwrap_or("text")
                        )
                    },
                );
                writeln!(
                    output,
                    "- `{}` — {} commits; {}",
                    utils::escape_inline_code(&path.path),
                    path.commits,
                    normalized
                )
                .expect("writing to a string cannot fail");
            }
        }
        writeln!(output, "Size basis: {}", utils::sanitize_text(&churn.size_basis))
            .expect("writing to a string cannot fail");
        writeln!(
            output,
            "Rename continuity: {} — {}",
            utils::sanitize_text(&churn.rename_continuity.status),
            utils::sanitize_text(&churn.rename_continuity.detail)
        )
        .expect("writing to a string cannot fail");
        Render::caveats(output, &churn.caveats);
    }

    fn contributors_markdown(output: &mut String, contributors: &super::ContributorReport) {
        Render::section_heading(output, "Contributor concentration");
        writeln!(output, "Committed .mailmap applied: {}", contributors.mailmap_applied)
            .expect("writing to a string cannot fail");
        if !contributors.identity_mappings.is_empty() {
            writeln!(
                output,
                "Canonicalized identities: {}",
                contributors.identity_mappings.len()
            )
            .expect("writing to a string cannot fail");
        }
        Render::contributors_group(output, "All non-merge commits", &contributors.overall);
        Render::contributors_group(output, "Recent non-merge commits", &contributors.recent);
        Render::caveats(output, &contributors.caveats);
    }

    fn contributors_group(output: &mut String, label: &str, contributors: &[super::ContributorCount]) {
        writeln!(output, "#### {label}").expect("writing to a string cannot fail");
        if contributors.is_empty() {
            writeln!(output, "No contributors were found.").expect("writing to a string cannot fail");
            return;
        }
        for contributor in contributors {
            let identity = contributor.email.as_ref().map_or_else(
                || utils::sanitize_text(&contributor.name),
                |email| {
                    format!(
                        "{} <{}>",
                        utils::sanitize_text(&contributor.name),
                        utils::sanitize_text(email)
                    )
                },
            );
            writeln!(
                output,
                "- {} — {} commits ({}%)",
                identity, contributor.commits, contributor.share_percent
            )
            .expect("writing to a string cannot fail");
        }
    }

    fn bugs_markdown(output: &mut String, bugs: &super::BugReport) {
        Render::section_heading(output, "Bug-related clusters");
        writeln!(output, "Window: {} days", bugs.window_days).expect("writing to a string cannot fail");
        writeln!(
            output,
            "Keywords ({} matching): {}",
            bugs.keyword_match.label(),
            utils::inline_code_list(&bugs.keywords)
        )
        .expect("writing to a string cannot fail");
        Render::paths(output, "Bug-related paths", &bugs.paths);
        Render::paths(output, "Churn overlap", &bugs.overlap_paths);
        Render::commits(output, &bugs.commits);
        Render::caveats(output, &bugs.caveats);
    }

    fn activity_markdown(output: &mut String, activity: &super::ActivityReport) {
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

    fn firefighting_markdown(output: &mut String, firefighting: &super::FirefightingReport) {
        Render::section_heading(output, "Firefighting commits");
        writeln!(
            output,
            "Window: {} days; keywords ({} matching): {}",
            firefighting.window_days,
            firefighting.keyword_match.label(),
            utils::inline_code_list(&firefighting.keywords)
        )
        .expect("writing to a string cannot fail");
        Render::commits(output, &firefighting.commits);
        Render::caveats(output, &firefighting.caveats);
    }

    fn paths(output: &mut String, label: &str, paths: &[super::PathCount]) {
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
