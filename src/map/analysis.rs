use super::*;

pub fn analyze(path: &Path, settings: &MapSettings) -> Result<MapReport> {
    let selected_path = absolute_path(path)?;
    let repository = security::discover_repository(&selected_path)
        .map_err(|source| MapError::Discovery { path: selected_path.clone(), source })?;
    let scope = security::resolve_scope(&repository, &selected_path).map_err(|error| match error {
        security::ScopeError::Input(reason) => MapError::Input { path: selected_path.clone(), reason },
        security::ScopeError::Safety(error) => MapError::safety("resolving the analysis scope", error),
    })?;
    let head = repository_head_snapshot(&repository)?;
    let repository_root = &scope.repository_root;
    let limits = ReportLimits::for_profile(settings.profile);
    let analysis_started = Instant::now();

    let exclusions = build_exclusions(repository_root, &settings.excludes)?;
    if settings.map_tokens == 0 {
        return Err(MapError::Input {
            path: selected_path,
            reason: "map token budget must be greater than zero".to_owned(),
        });
    }
    if settings.cache_mode == CacheMode::Files && settings.cache_files.is_empty() {
        return Err(MapError::Input {
            path: selected_path,
            reason: "files cache mode requires at least one changed-file path".to_owned(),
        });
    }
    let cache = if settings.cache_mode == CacheMode::Disabled {
        CacheStore {
            root: None,
            repository_root: repository_root.to_string_lossy().into_owned(),
            repository_id: digest_hex(repository_root.to_string_lossy().as_bytes()),
        }
    } else {
        CacheStore::new(repository_root)?
    };
    let mut cache_stats = CacheStats::default();
    let mut cache_limitations = Vec::new();
    if settings.cache_mode != CacheMode::Disabled && cache.root.is_none() {
        cache_limitations.push(
            "The XDG cache location could not be resolved; source analysis continued without persistent cache data."
                .to_owned(),
        );
    }
    let mut tracked_paths = BTreeSet::new();
    let mut submodule_paths = BTreeSet::new();
    let mut classification_records = Vec::new();
    let tracked_tree_truncated = collect_tree_files(
        &repository,
        &repository
            .head_tree_id_or_empty()
            .map_err(|error| MapError::analysis("resolving the repository HEAD tree", error))?,
        b"",
        &mut TreeCollection {
            files: &mut tracked_paths,
            submodule_paths: &mut submodule_paths,
            classification_records: &mut classification_records,
            max_files: limits.max_files,
            max_depth: limits.max_syntax_depth,
            focus_paths: &settings.focus_paths,
        },
    )?;
    let tracked_index_truncated = collect_index_files(
        &repository,
        &mut tracked_paths,
        limits.max_files,
        &settings.focus_paths,
        &mut classification_records,
    )?;
    let modified_paths = collect_modified_paths(
        &repository,
        repository_root,
        limits.max_file_bytes,
        &settings.focus_paths,
    )?;

    let mut candidates = BTreeMap::new();
    for path in tracked_paths
        .into_iter()
        .filter(|path| in_scope(path, &scope.relative_path) && !submodule_paths.contains(path))
    {
        let state = if modified_paths.contains(&path) { WorktreeState::Modified } else { WorktreeState::Tracked };
        candidates.insert(path, Candidate { state, symlink: false });
    }

    let (visible_paths, visible_errors, visible_classified_directories) = walk_files(
        &scope.selected_path,
        repository_root,
        true,
        limits.max_files,
        settings.recursive,
        true,
        &settings.focus_paths,
    );
    for (path, symlink) in &visible_paths {
        if is_git_internal(path) || !in_scope(path, &scope.relative_path) {
            continue;
        }
        candidates
            .entry(path.clone())
            .and_modify(|candidate| candidate.symlink |= *symlink)
            .or_insert(Candidate { state: WorktreeState::Untracked, symlink: *symlink });
    }

    let (all_paths, all_errors, all_classified_directories) = walk_files(
        &scope.selected_path,
        repository_root,
        false,
        limits.max_files,
        settings.recursive,
        true,
        &settings.focus_paths,
    );
    let visible_path_set: BTreeSet<_> = visible_paths.keys().cloned().collect();
    let mut omissions = Vec::new();
    if tracked_tree_truncated || tracked_index_truncated {
        omissions.push(omission(
            scope.relative_path.clone(),
            OmissionReason::TraversalError,
            format!(
                "The tracked-file inventory reached the {}-path resource limit before every tracked path could be inspected.",
                limits.max_files
            ),
        ));
    }
    for error in visible_errors.into_iter().chain(all_errors) {
        let (reason, detail) = match error {
            WalkIssue::Traversal(detail) => (OmissionReason::TraversalError, detail),
            WalkIssue::Safety(detail) => (OmissionReason::UnsafePath, detail),
        };
        omissions.push(omission(scope.relative_path.clone(), reason, detail));
    }
    for directory in visible_classified_directories
        .into_iter()
        .chain(all_classified_directories)
    {
        record_classification(
            &mut classification_records,
            &directory.path,
            &directory.classifications,
            false,
        );
        if !omissions.iter().any(|omission| omission.path == directory.path) {
            omissions.push(classified_omission(directory.path, directory.classifications, false));
        }
    }
    for (path, symlink) in all_paths {
        if is_git_internal(&path)
            || !in_scope(&path, &scope.relative_path)
            || candidates.contains_key(&path)
            || visible_path_set.contains(&path)
        {
            continue;
        }
        let classifications = classified_path(&path);
        if !classifications.is_empty() && !symlink {
            let overridden = classification_override(&path, settings);
            record_classification(&mut classification_records, &path, &classifications, overridden);
            if overridden {
                candidates.insert(path, Candidate { state: WorktreeState::Untracked, symlink: false });
            } else {
                omissions.push(classified_omission(path, classifications, false));
            }
            continue;
        }
        let detail = if symlink {
            "The ignored untracked symlink was inventoried without following its target."
        } else {
            "The untracked Rust file was omitted by the ignore crate traversal policy."
        };
        omissions.push(omission(path, OmissionReason::IgnoredUntracked, detail));
    }

    let requested_cache_files = settings
        .cache_files
        .iter()
        .filter_map(|path| normalized_cache_file_path(path, repository_root))
        .collect::<BTreeSet<_>>();
    if settings.cache_mode == CacheMode::Files {
        let eligible_cache_paths = candidates
            .iter()
            .filter(|(path, candidate)| {
                support_for_path(Path::new(path)).is_some()
                    && !candidate.symlink
                    && !explicitly_excluded(exclusions.as_ref(), &repository_root.join(path))
                    && !fs::symlink_metadata(repository_root.join(path))
                        .map(|metadata| metadata.file_type().is_symlink())
                        .unwrap_or(false)
            })
            .map(|(path, _)| path.clone())
            .collect::<BTreeSet<_>>();
        cache_stats.matched = requested_cache_files
            .iter()
            .filter(|path| eligible_cache_paths.contains(*path))
            .count();
        cache_stats.unmatched = requested_cache_files.len().saturating_sub(cache_stats.matched);
    }

    let landmark_states = candidates
        .iter()
        .map(|(path, candidate)| (path.clone(), candidate.state))
        .collect::<BTreeMap<_, _>>();
    let topology = landmarks::analyze(landmarks::LandmarkAnalysisOptions {
        repository_root,
        scope_root: &scope.selected_path,
        scope_path: &scope.relative_path,
        path_states: &landmark_states,
        submodule_paths: &submodule_paths,
        exclusions: exclusions.as_ref(),
        limits: &limits,
        recursive: settings.recursive,
        focuses: &settings.focuses,
        focus_paths: &settings.focus_paths,
    });
    let inventory = inventory(&candidates);
    if candidates.len() > limits.max_files {
        let kept = candidates
            .keys()
            .take(limits.max_files)
            .cloned()
            .collect::<BTreeSet<_>>();
        for path in candidates.keys().filter(|path| !kept.contains(*path)) {
            omissions.push(omission(
                path.clone(),
                OmissionReason::TraversalError,
                format!(
                    "The file-count resource limit ({}) was reached before this path could be analyzed.",
                    limits.max_files
                ),
            ));
        }
        candidates.retain(|path, _| kept.contains(path));
    }
    let mut files = Vec::new();
    let mut findings = Vec::new();
    let mut total_source_bytes = 0usize;
    let mut total_symbols = 0usize;
    let mut work_limit_reached = false;
    let mut analyzers = BTreeMap::<SourceLanguage, LanguageAnalyzer>::new();
    for (path, candidate) in candidates {
        if analysis_started.elapsed().as_millis() >= u128::from(limits.max_elapsed_ms) {
            omissions.push(omission(
                scope.relative_path.clone(),
                OmissionReason::TraversalError,
                format!(
                    "The analysis time resource limit ({} ms) was reached before this path could be analyzed.",
                    limits.max_elapsed_ms
                ),
            ));
            work_limit_reached = true;
            break;
        }
        let absolute = repository_root.join(&path);
        if explicitly_excluded(exclusions.as_ref(), &absolute) {
            omissions.push(omission(
                path,
                OmissionReason::ExplicitExclusion,
                "The caller supplied an exclusion glob for this path.",
            ));
            continue;
        }
        let symlink = candidate.symlink
            || fs::symlink_metadata(&absolute)
                .map(|metadata| metadata.file_type().is_symlink())
                .unwrap_or(false);
        if symlink {
            omissions.push(omission(
                path,
                OmissionReason::Symlink,
                "Symlinked source paths are omitted so map traversal cannot follow a path outside the requested scope.",
            ));
            continue;
        }
        let overridden = classification_override(&path, settings);
        let mut classifications = classified_path(&path);
        if !classifications.is_empty() {
            record_classification(&mut classification_records, &path, &classifications, overridden);
            if !overridden {
                omissions.push(classified_omission(path, classifications, false));
                continue;
            }
        }
        let path_support = support_for_path(Path::new(&path));
        if path_support.is_none() && !is_extensionless_lua_entry_candidate(Path::new(&path)) {
            let source_like = is_source_like_path(Path::new(&path));
            let mut path_omission = omission(
                path,
                if source_like { OmissionReason::UnsupportedLanguage } else { OmissionReason::NonSource },
                if source_like {
                    "The source-language extension is not registered with a first-class parser; the path was not parsed."
                } else {
                    "The path is not a registered source-language file; it was inventoried but not parsed."
                },
            );
            path_omission.classifications = classifications;
            path_omission.classification_overridden = overridden;
            omissions.push(path_omission);
            continue;
        }
        let source = match security::read_worktree_file_limited(
            repository_root,
            &scope.selected_path,
            &path,
            limits.max_file_bytes,
        ) {
            Ok(source) => source,
            Err(security::ReadError::Safety(error)) => {
                let reason = if matches!(error.kind, security::PathSafetyKind::Symlink) {
                    OmissionReason::Symlink
                } else {
                    OmissionReason::UnsafePath
                };
                let mut path_omission = omission(path, reason, error.to_string());
                path_omission.classifications = classifications;
                path_omission.classification_overridden = overridden;
                omissions.push(path_omission);
                continue;
            }
            Err(error) => {
                let reason = if matches!(
                    &error,
                    security::ReadError::Io(io_error) if io_error.kind() == std::io::ErrorKind::InvalidData
                ) {
                    OmissionReason::Oversized
                } else {
                    OmissionReason::ReadError
                };
                let mut path_omission = omission(path, reason, error.to_string());
                path_omission.classifications = classifications;
                path_omission.classification_overridden = overridden;
                omissions.push(path_omission);
                continue;
            }
        };
        if source.contains(&0) {
            let mut path_omission = omission(
                path,
                OmissionReason::Binary,
                "Binary or non-UTF-8 source input is not parsed by the Tree-sitter map.",
            );
            path_omission.classifications = classifications;
            path_omission.classification_overridden = overridden;
            omissions.push(path_omission);
            continue;
        }
        let source_text = match std::str::from_utf8(&source) {
            Ok(source_text) => source_text,
            Err(error) => {
                let mut path_omission = omission(path, OmissionReason::Binary, error.to_string());
                path_omission.classifications = classifications;
                path_omission.classification_overridden = overridden;
                omissions.push(path_omission);
                continue;
            }
        };
        let Some(support) = path_support.or_else(|| lua_support_for_entry_source(Path::new(&path), source_text)) else {
            let mut path_omission = omission(
                path,
                OmissionReason::NonSource,
                "The extensionless entry file does not use a recognized Lua or LuaJIT shebang; it was inventoried but not parsed.",
            );
            path_omission.classifications = classifications;
            path_omission.classification_overridden = overridden;
            omissions.push(path_omission);
            continue;
        };
        classifications.extend(source_classifications(&path, source_text));
        classifications.sort_by(|left, right| left.kind.cmp(&right.kind).then_with(|| left.reason.cmp(&right.reason)));
        classifications.dedup();
        if !classifications.is_empty() {
            record_classification(&mut classification_records, &path, &classifications, overridden);
            if !overridden {
                omissions.push(classified_omission(path, classifications, false));
                continue;
            }
        }
        if total_source_bytes.saturating_add(source.len()) > limits.max_total_bytes {
            let mut path_omission = omission(
                path,
                OmissionReason::Oversized,
                format!(
                    "The total source-byte resource limit ({}) was reached; this file was not analyzed.",
                    limits.max_total_bytes
                ),
            );
            path_omission.classifications = classifications;
            path_omission.classification_overridden = overridden;
            omissions.push(path_omission);
            continue;
        }
        total_source_bytes = total_source_bytes.saturating_add(source.len());
        let fingerprint = source_fingerprint(&source);
        let requested = requested_cache_files.contains(&path);
        let forced_refresh =
            matches!(settings.cache_mode, CacheMode::Always) || (settings.cache_mode == CacheMode::Files && requested);
        let (parsed, stale) = if settings.cache_mode == CacheMode::Disabled || forced_refresh {
            let analyzer = analyzers
                .entry(support.language)
                .or_insert_with(|| LanguageAnalyzer::new(support));
            let parsed = parse_source_with_analyzer(&source, analyzer, &limits);
            if settings.cache_mode != CacheMode::Disabled {
                if let Some(error) = cache.write(&path, support, &fingerprint, &parsed) {
                    cache_limitations.push(error);
                }
                cache_stats.refreshed.push(path.clone());
            }
            (parsed, false)
        } else {
            let lookup_mode =
                if settings.cache_mode == CacheMode::Files { CacheMode::Auto } else { settings.cache_mode };
            match cache.load(&path, support, &fingerprint, lookup_mode) {
                Some(lookup) => {
                    cache_stats.hits += 1;
                    if lookup.stale {
                        cache_stats.stale.push(path.clone());
                    }
                    (lookup.parsed, lookup.stale)
                }
                None => {
                    cache_stats.misses += 1;
                    if matches!(settings.cache_mode, CacheMode::Manual | CacheMode::Files) {
                        cache_stats.unavailable += 1;
                        let mut path_omission = omission(
                            path.clone(),
                            OmissionReason::CacheUnavailable,
                            if settings.cache_mode == CacheMode::Manual {
                                "Manual cache mode found no usable record for this file and did not refresh it."
                            } else {
                                "Files cache mode did not refresh this unrequested file because no current cache record was available."
                            },
                        );
                        path_omission.classifications = classifications.clone();
                        path_omission.classification_overridden = overridden;
                        omissions.push(path_omission);
                        continue;
                    }
                    let analyzer = analyzers
                        .entry(support.language)
                        .or_insert_with(|| LanguageAnalyzer::new(support));
                    let parsed = parse_source_with_analyzer(&source, analyzer, &limits);
                    if let Some(error) = cache.write(&path, support, &fingerprint, &parsed) {
                        cache_limitations.push(error);
                    }
                    cache_stats.refreshed.push(path.clone());
                    (parsed, false)
                }
            }
        };
        let ParsedSource { mut symbols, findings: file_findings, status, mut limitations } = parsed;
        let available_symbols = limits.max_symbols.saturating_sub(total_symbols);
        if symbols.len() > available_symbols {
            symbols.truncate(available_symbols);
            limitations.push(format!(
                "The symbol resource limit ({}) was reached; additional symbols were omitted from this report.",
                limits.max_symbols
            ));
        }
        total_symbols = total_symbols.saturating_add(symbols.len());
        if stale {
            limitations.push(
                "Manual cache mode used a potentially stale source analysis; rerun with `--cache always` to refresh it."
                    .to_owned(),
            );
        }
        let finding_limit = limits.max_findings.saturating_sub(findings.len());
        findings.extend(file_findings.into_iter().take(finding_limit).map(|mut finding| {
            if finding.path.is_empty() {
                finding.path = path.clone();
            }
            finding
        }));
        let extension = extension_for_path(Path::new(&path));
        files.push(SourceFile {
            path,
            language: support.language,
            extension,
            worktree_state: candidate.state,
            status,
            symbols,
            limitations,
            classifications,
            classification_overridden: overridden,
        });
    }

    files.sort_by(|left, right| left.path.cmp(&right.path));
    omissions.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.reason.label().cmp(right.reason.label()))
    });
    findings.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| location_key(left.location.as_ref()).cmp(&location_key(right.location.as_ref())))
            .then_with(|| left.kind.label().cmp(right.kind.label()))
            .then_with(|| left.detail.cmp(&right.detail))
    });

    let query_packs = supported_query_packs(&files);
    let query_pack = if query_packs.len() == 1 {
        query_packs
            .values()
            .next()
            .cloned()
            .unwrap_or_else(|| RUST_SUPPORT.query_pack.to_owned())
    } else {
        "mixed".to_owned()
    };

    let has_non_rust_files = files.iter().any(|file| file.language != SourceLanguage::Rust);
    let mut limitations = if has_non_rust_files {
        vec![
            "Definitions and references are extracted lexically with language-specific Tree-sitter queries; only explicit import/module evidence contributes cross-file edges, and types, macros, and runtime behavior are not semantically resolved."
                .to_owned(),
            "Reference names can have multiple lexical definition candidates; ambiguity is reported rather than treated as a semantic call edge."
                .to_owned(),
            "JavaScript/JSX, TypeScript/TSX, Python, Ruby, Java, C#, Go, and Lua use explicit grammar variants; query-pack provenance is reported per language."
                .to_owned(),
            "Tracked files are eligible even when ignore rules match them, except deterministic generated/vendor/minified classifications; exact focus paths can opt in within the safety limits."
                .to_owned(),
        ]
    } else {
        vec![
            "Rust definitions and references are extracted lexically; only explicit same-file call evidence is graphed, and imports, types, macros, and runtime behavior are not semantically resolved."
                .to_owned(),
            "Reference names can have multiple lexical definition candidates; ambiguity is reported rather than treated as a semantic call edge."
                .to_owned(),
            "Tracked files are eligible even when ignore rules match them, except deterministic generated/vendor/minified classifications; exact focus paths can opt in within the safety limits."
                .to_owned(),
        ]
    };

    if !files.is_empty() {
        limitations.push(
            "Ranking uses deterministic lexical centrality; generic and underscore-prefixed names are downweighted only for ranking and remain available in the full symbol evidence."
                .to_owned(),
        );
    }
    limitations.extend(cache_limitations);
    if work_limit_reached {
        limitations.push(format!(
            "Source analysis stopped at the {} ms elapsed-work limit; the returned map is partial.",
            limits.max_elapsed_ms
        ));
    }

    let edges = build_lexical_edges(&files, limits.max_candidates_per_reference, limits.max_edges);
    add_ambiguity_findings(&edges, &mut findings, limits.max_findings);
    findings.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| location_key(left.location.as_ref()).cmp(&location_key(right.location.as_ref())))
            .then_with(|| left.kind.label().cmp(right.kind.label()))
            .then_with(|| left.detail.cmp(&right.detail))
    });
    let ranking = rank_files(&files, &edges, settings);
    let selection_budget = if settings.profile == AnalysisProfile::Evidence || settings.map_tokens < 20 {
        settings.map_tokens
    } else {
        settings.map_tokens.saturating_mul(2).div_ceil(3).max(1)
    };
    let mut selection = select_snippets(&files, &edges, &ranking, selection_budget, settings);
    selection.token_budget = settings.map_tokens;
    let cache_status = cache_status(settings.cache_mode, &cache_stats);
    cache_stats.refreshed.sort();
    cache_stats.stale.sort();

    let mut project_roots = topology.project_roots;
    let root_paths = project_roots.clone();
    for root in &mut project_roots {
        root.landmark_total = topology
            .landmarks
            .iter()
            .filter(|landmark| landmark.project_root.as_deref() == Some(root.path.as_str()))
            .count();
        root.recommended_paths = ranking
            .iter()
            .filter(|rank| {
                landmarks::project_root_for_path(&rank.path, &root_paths).as_deref() == Some(root.path.as_str())
            })
            .map(|rank| rank.path.clone())
            .collect();
        root.recommendation_total = root.recommended_paths.len();
    }
    let reading_evidence = ReadingPlanEvidence {
        sources: files
            .iter()
            .map(|file| ReadingSourceEvidence { path: file.path.clone(), limitations: file.limitations.clone() })
            .collect(),
        ranking: ranking.clone(),
        graph: edges
            .iter()
            .map(|edge| ReadingGraphEvidence { source: edge.source.clone(), target: edge.target.clone() })
            .collect(),
        omissions: omissions.clone(),
        landmarks: topology.landmarks.clone(),
        project_roots: project_roots.clone(),
    };
    let resource_limited = omissions.iter().any(|omission| {
        matches!(
            omission.reason,
            OmissionReason::TraversalError | OmissionReason::Oversized
        )
    }) || files.iter().any(|file| {
        file.limitations
            .iter()
            .any(|limitation| actionable_resource_limit(limitation))
    });
    let unsupported_path_names = omissions
        .iter()
        .filter(|omission| omission.reason == OmissionReason::UnsupportedLanguage)
        .map(|omission| omission.path.clone())
        .collect::<Vec<_>>();
    let partial_path_names = files
        .iter()
        .filter(|file| file.status == FileAnalysisStatus::Partial)
        .map(|file| file.path.clone())
        .collect::<Vec<_>>();
    let cache_unavailable_path_names = omissions
        .iter()
        .filter(|omission| omission.reason == OmissionReason::CacheUnavailable)
        .map(|omission| omission.path.clone())
        .collect::<Vec<_>>();
    let mut report = MapReport {
        profile: settings.profile,
        repository_root: repository_root.to_string_lossy().into_owned(),
        scope_path: scope.relative_path,
        head,
        worktree: worktree_snapshot(inventory),
        query_pack,
        query_packs,
        exclusions: settings.excludes.clone(),
        inventory: MapInventory {
            tracked: inventory.0,
            modified: inventory.1,
            untracked: inventory.2,
            analyzed: files.len(),
            omitted: omissions.len(),
        },
        classifications: classification_summary(&mut classification_records),
        availability: MapAvailability {
            unsupported_paths: unsupported_path_names.len(),
            partial_files: partial_path_names.len(),
            cache_unavailable_paths: cache_unavailable_path_names.len(),
            resource_limited,
            unsafe_paths: omissions
                .iter()
                .filter(|omission| matches!(omission.reason, OmissionReason::UnsafePath | OmissionReason::Symlink))
                .count(),
            unsupported_path_names,
            partial_path_names,
            cache_unavailable_path_names,
        },
        files,
        omissions,
        findings,
        limitations: {
            let mut combined = limitations;
            combined.extend(topology.limitations);
            combined
        },
        edges,
        ranking,
        selection,
        cache: MapCacheReport {
            mode: settings.cache_mode,
            status: cache_status,
            hits: cache_stats.hits,
            misses: cache_stats.misses,
            matched: cache_stats.matched,
            unmatched: cache_stats.unmatched,
            unavailable: cache_stats.unavailable,
            refreshed: cache_stats.refreshed,
            stale: cache_stats.stale,
        },
        landmarks: topology.landmarks,
        project_roots,
        collections: MapCollections {
            files: CollectionSummary::complete(0),
            symbols: CollectionSummary::complete(0),
            omissions: CollectionSummary::complete(0),
            findings: CollectionSummary::complete(0),
            edges: CollectionSummary::complete(0),
            ranking: CollectionSummary::complete(0),
            snippets: CollectionSummary::complete(0),
            landmarks: CollectionSummary::complete(0),
            project_roots: CollectionSummary::complete(0),
        },
        reading_evidence,
    };
    bound_map_report(&mut report, settings.profile, &limits);
    Ok(report)
}

fn actionable_resource_limit(limitation: &str) -> bool {
    !limitation.starts_with("The per-file symbol limit")
        && (limitation.contains("resource limit") || limitation.starts_with("Syntax traversal reached the depth limit"))
}

pub fn bound_map_report(report: &mut MapReport, profile: AnalysisProfile, limits: &ReportLimits) {
    let files_total = report.files.len();
    let symbols_total = report.files.iter().map(|file| file.symbols.len()).sum::<usize>();
    let omissions_total = report.omissions.len();
    let findings_total = report.findings.len();
    let edges_total = report.edges.len();
    let ranking_total = report.ranking.len();
    let snippets_total = report.selection.snippets.len();
    let landmarks_total = report.landmarks.len();
    let project_roots_total = report.project_roots.len();
    let (
        file_limit,
        symbols_per_file,
        omission_limit,
        finding_limit,
        edge_limit,
        ranking_limit,
        landmark_limit,
        root_limit,
    ) = match profile {
        AnalysisProfile::Compact => (32, 16, 8, 32, 32, 16, 48, 16),
        AnalysisProfile::Evidence => (
            limits.max_files,
            limits.max_symbols_per_file,
            limits.max_findings,
            limits.max_findings,
            limits.max_edges,
            limits.max_files,
            limits.max_landmarks,
            limits.max_project_roots,
        ),
    };

    report.files.truncate(file_limit);
    let mut remaining_symbols = if profile == AnalysisProfile::Compact { 128 } else { limits.max_symbols };
    for file in &mut report.files {
        file.symbols.truncate(symbols_per_file.min(remaining_symbols));
        remaining_symbols = remaining_symbols.saturating_sub(file.symbols.len());
        for limitation in &mut file.limitations {
            *limitation = bounded_text(limitation, 512);
        }
    }
    report.omissions.truncate(omission_limit);
    for omission in &mut report.omissions {
        omission.detail = bounded_text(&omission.detail, 256);
    }
    report.findings.truncate(finding_limit);
    for finding in &mut report.findings {
        finding.detail = bounded_text(&finding.detail, 256);
    }
    report.edges.truncate(edge_limit);
    for edge in &mut report.edges {
        edge.candidates.truncate(limits.max_candidates_per_reference);
    }
    report.ranking.truncate(ranking_limit);
    report.landmarks.truncate(landmark_limit);
    report.project_roots.truncate(root_limit);
    for root in &mut report.project_roots {
        let recommendation_limit = if profile == AnalysisProfile::Compact { 16 } else { limits.max_files };
        root.recommended_paths.truncate(recommendation_limit);
    }

    let enforce_budget = profile == AnalysisProfile::Compact
        && (report.selection.token_budget < 256
            || files_total > 16
            || symbols_total > 256
            || omissions_total > 32
            || findings_total > 128
            || edges_total > 256);

    if enforce_budget {
        // The selection is the highest-value compact evidence. Other fields
        // are reduced until the same requested budget accounts for every
        // remaining data-dependent field in the map.
        while report.compact_payload_tokens() > report.selection.token_budget {
            if report.findings.len() > 1 {
                report.findings.pop();
            } else if report.selection.snippets.len() > 1 {
                report.selection.snippets.pop();
            } else if report.edges.len() > 1 {
                report.edges.pop();
            } else if report.ranking.len() > 1 {
                report.ranking.pop();
            } else if report.files.len() > 1 {
                report.files.pop();
            } else if report.omissions.len() > 1 {
                report.omissions.pop();
            } else {
                report.findings.clear();
                report.omissions.clear();
                if report.selection.token_budget < 20 {
                    report.edges.clear();
                    report.ranking.clear();
                } else {
                    report.selection.snippets.clear();
                }
                break;
            }
        }
    }

    report.selection.estimated_tokens = if enforce_budget {
        report.compact_payload_tokens().min(report.selection.token_budget)
    } else {
        report
            .selection
            .snippets
            .iter()
            .map(|snippet| snippet.estimated_tokens)
            .sum()
    };
    report.collections = MapCollections {
        files: collection_summary(
            files_total,
            report.files.len(),
            profile,
            TruncationReason::CollectionLimit,
        ),
        symbols: collection_summary(
            symbols_total,
            report.files.iter().map(|file| file.symbols.len()).sum(),
            profile,
            TruncationReason::CollectionLimit,
        ),
        omissions: collection_summary(
            omissions_total,
            report.omissions.len(),
            profile,
            TruncationReason::CollectionLimit,
        ),
        findings: collection_summary(
            findings_total,
            report.findings.len(),
            profile,
            TruncationReason::CollectionLimit,
        ),
        edges: collection_summary(
            edges_total,
            report.edges.len(),
            profile,
            TruncationReason::CollectionLimit,
        ),
        ranking: collection_summary(
            ranking_total,
            report.ranking.len(),
            profile,
            TruncationReason::CollectionLimit,
        ),
        snippets: collection_summary(
            snippets_total,
            report.selection.snippets.len(),
            profile,
            TruncationReason::OutputBudget,
        ),
        landmarks: collection_summary(
            landmarks_total,
            report.landmarks.len(),
            profile,
            TruncationReason::CollectionLimit,
        ),
        project_roots: collection_summary(
            project_roots_total,
            report.project_roots.len(),
            profile,
            TruncationReason::CollectionLimit,
        ),
    };
    if profile == AnalysisProfile::Evidence {
        bound_evidence_output(report, 4 * 1_024 * 1_024);
    }
    if [
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
    .any(|truncated| truncated)
    {
        report.limitations.push(
            "The emitted map is a bounded sample; collection totals and truncation reasons identify evidence that was not returned."
                .to_owned(),
        );
    }
}

fn bound_evidence_output(report: &mut MapReport, max_json_bytes: usize) {
    while serde_json::to_vec(&*report).is_ok_and(|json| json.len() > max_json_bytes) {
        let symbols_before = report.files.iter().map(|file| file.symbols.len()).sum::<usize>();
        for file in &mut report.files {
            if file.symbols.len() > 16 {
                file.symbols.truncate(file.symbols.len().div_ceil(2).max(16));
            }
        }
        let symbols_after = report.files.iter().map(|file| file.symbols.len()).sum::<usize>();
        if symbols_after < symbols_before {
            continue;
        }

        let mut changed = false;
        changed |= truncate_half(&mut report.omissions, 16);
        changed |= truncate_half(&mut report.findings, 16);
        changed |= truncate_half(&mut report.edges, 16);
        changed |= truncate_half(&mut report.ranking, 16);
        changed |= truncate_half(&mut report.selection.snippets, 16);
        changed |= truncate_half(&mut report.landmarks, 16);
        changed |= truncate_half(&mut report.project_roots, 8);
        if !changed {
            changed = truncate_half(&mut report.files, 8);
        }
        if !changed {
            break;
        }
    }

    update_output_summary(&mut report.collections.files, report.files.len());
    update_output_summary(
        &mut report.collections.symbols,
        report.files.iter().map(|file| file.symbols.len()).sum(),
    );
    update_output_summary(&mut report.collections.omissions, report.omissions.len());
    update_output_summary(&mut report.collections.findings, report.findings.len());
    update_output_summary(&mut report.collections.edges, report.edges.len());
    update_output_summary(&mut report.collections.ranking, report.ranking.len());
    update_output_summary(&mut report.collections.snippets, report.selection.snippets.len());
    update_output_summary(&mut report.collections.landmarks, report.landmarks.len());
    update_output_summary(&mut report.collections.project_roots, report.project_roots.len());
}

fn truncate_half<T>(items: &mut Vec<T>, minimum: usize) -> bool {
    if items.len() <= minimum {
        return false;
    }
    let previous = items.len();
    items.truncate(items.len().div_ceil(2).max(minimum));
    items.len() < previous
}

fn update_output_summary(summary: &mut CollectionSummary, returned: usize) {
    let output_truncated = returned < summary.returned;
    summary.returned = returned.min(summary.total);
    summary.truncated = summary.returned < summary.total;
    if output_truncated {
        summary.reason = Some(TruncationReason::OutputBudget);
    }
}

pub fn collection_summary(
    total: usize, returned: usize, _profile: AnalysisProfile, reason: TruncationReason,
) -> CollectionSummary {
    if returned >= total {
        CollectionSummary::complete(total)
    } else {
        let reason = if reason == TruncationReason::CollectionLimit {
            TruncationReason::ProfileProjection
        } else {
            reason
        };
        CollectionSummary { total, returned, truncated: true, reason: Some(reason) }
    }
}

pub fn bounded_text(text: &str, max_chars: usize) -> String {
    let mut output = text.chars().take(max_chars).collect::<String>();
    if text.chars().count() > max_chars {
        output.push('…');
    }
    output
}
