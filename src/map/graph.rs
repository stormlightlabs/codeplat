use super::*;

pub fn build_lexical_edges(files: &[SourceFile], max_candidates: usize, max_edges: usize) -> Vec<LexicalEdge> {
    let mut definitions = BTreeMap::<(SourceLanguage, String), Vec<(String, SymbolVisibility)>>::new();
    for file in files {
        for symbol in &file.symbols {
            if is_graph_definition(symbol) {
                definitions
                    .entry((file.language, symbol.name.clone()))
                    .or_default()
                    .push((file.path.clone(), symbol.visibility));
            }
        }
    }
    for candidates in definitions.values_mut() {
        candidates.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.label().cmp(right.1.label())));
        candidates.dedup_by(|right, left| right.0 == left.0);
    }

    let imports = files
        .iter()
        .map(|file| {
            (
                file.path.clone(),
                file.symbols
                    .iter()
                    .filter(|symbol| symbol.role == SymbolRole::Definition && symbol.evidence == SymbolEvidence::Import)
                    .map(|symbol| (symbol.name.clone(), import_module_hints(&symbol.context, file.language)))
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let modules = files
        .iter()
        .map(|file| {
            (
                file.path.clone(),
                file.symbols
                    .iter()
                    .filter(|symbol| symbol.role == SymbolRole::Definition && symbol.kind == SymbolKind::Module)
                    .map(|symbol| symbol.name.clone())
                    .collect::<BTreeSet<_>>(),
            )
        })
        .collect::<BTreeMap<_, _>>();

    let mut edges = Vec::new();
    'files: for file in files {
        for symbol in &file.symbols {
            if edges.len() >= max_edges {
                break 'files;
            }
            if !is_graph_reference(symbol) {
                continue;
            }
            let Some(all_candidates) = definitions.get(&(file.language, symbol.name.clone())) else {
                continue;
            };
            let same_file = all_candidates
                .iter()
                .filter(|(path, _)| path == &file.path)
                .cloned()
                .collect::<Vec<_>>();
            let file_imports = imports.get(&file.path).into_iter().flatten().collect::<Vec<_>>();
            let exact_import = file_imports.iter().find(|(name, _)| name == &symbol.name);
            let (candidates, reason, confidence) = if !same_file.is_empty() {
                if symbol.evidence != SymbolEvidence::BareReference {
                    (
                        same_file,
                        LexicalResolutionReason::SameFileExplicit,
                        ConfidenceTier::High,
                    )
                } else {
                    continue;
                }
            } else if let Some(module_candidates) = same_module_candidates(file, all_candidates, &modules) {
                (
                    module_candidates,
                    LexicalResolutionReason::SameModule,
                    ConfidenceTier::High,
                )
            } else {
                let imported_module_candidates = all_candidates
                    .iter()
                    .filter(|(path, _)| file_imports.iter().any(|(_, hints)| module_path_matches(path, hints)))
                    .cloned()
                    .collect::<Vec<_>>();
                if !imported_module_candidates.is_empty() {
                    (
                        imported_module_candidates,
                        LexicalResolutionReason::ImportedModule,
                        ConfidenceTier::High,
                    )
                } else {
                    let Some(_) = exact_import else {
                        // A cross-file name without package or import evidence is not a dependency.
                        continue;
                    };
                    (
                        all_candidates.clone(),
                        LexicalResolutionReason::ImportedName,
                        ConfidenceTier::Medium,
                    )
                }
            };
            let candidates = candidates.into_iter().take(max_candidates).collect::<Vec<_>>();
            if candidates.is_empty() {
                continue;
            }
            let candidate_paths = candidates.iter().map(|(path, _)| path.clone()).collect::<Vec<_>>();
            let candidate_group = format!(
                "{}:{}:{}:{}",
                file.path,
                symbol.name,
                reason.label(),
                digest_hex(candidate_paths.join("\0").as_bytes())
            );
            let ambiguous = candidates.len() > 1;
            for (target, target_visibility) in &candidates {
                if edges.len() >= max_edges {
                    break 'files;
                }
                edges.push(LexicalEdge {
                    source: file.path.clone(),
                    target: target.clone(),
                    symbol: symbol.name.clone(),
                    ambiguous,
                    candidates: candidate_paths.clone(),
                    candidate_group: candidate_group.clone(),
                    resolution_reason: reason,
                    confidence,
                    target_visibility: *target_visibility,
                });
            }
        }
    }
    edges.sort_by(|left, right| {
        left.source
            .cmp(&right.source)
            .then_with(|| left.target.cmp(&right.target))
            .then_with(|| left.symbol.cmp(&right.symbol))
            .then_with(|| left.ambiguous.cmp(&right.ambiguous))
            .then_with(|| left.candidate_group.cmp(&right.candidate_group))
    });
    edges.dedup();
    edges
}

pub fn rank_files(files: &[SourceFile], edges: &[LexicalEdge], settings: &MapSettings) -> Vec<FileRank> {
    if files.is_empty() {
        return Vec::new();
    }
    let paths = files.iter().map(|file| file.path.clone()).collect::<Vec<_>>();
    let path_set = paths.iter().cloned().collect::<BTreeSet<_>>();
    let mut outgoing = BTreeMap::<String, Vec<&LexicalEdge>>::new();
    for edge in edges {
        outgoing.entry(edge.source.clone()).or_default().push(edge);
    }

    let initial = 1.0 / paths.len() as f64;
    let mut scores = paths
        .iter()
        .map(|path| (path.clone(), initial))
        .collect::<BTreeMap<_, _>>();
    for _ in 0..PAGE_RANK_ITERATIONS {
        let mut next = paths
            .iter()
            .map(|path| (path.clone(), (1.0 - PAGE_RANK_DAMPING) * initial))
            .collect::<BTreeMap<_, _>>();
        let dangling = paths
            .iter()
            .filter(|path| outgoing.get(*path).is_none_or(Vec::is_empty))
            .map(|path| scores[path])
            .sum::<f64>();
        let dangling_share = PAGE_RANK_DAMPING * dangling * initial;
        for score in next.values_mut() {
            *score += dangling_share;
        }
        for source in &paths {
            let Some(source_edges) = outgoing.get(source) else {
                continue;
            };
            let total_weight = source_edges.iter().map(|edge| edge_weight(edge)).sum::<f64>();
            if total_weight == 0.0 {
                continue;
            }
            for edge in source_edges {
                if path_set.contains(&edge.target) {
                    let contribution = PAGE_RANK_DAMPING * scores[source] * edge_weight(edge) / total_weight;
                    *next.entry(edge.target.clone()).or_default() += contribution;
                }
            }
        }
        scores = next;
    }

    let mut ranking = files
        .iter()
        .map(|file| {
            let text_matches = settings
                .focuses
                .iter()
                .filter(|focus| file_matches_focus(file, focus))
                .count();
            let path_matches = settings
                .focus_paths
                .iter()
                .filter(|focus_path| path_matches_focus(&file.path, focus_path))
                .count();
            let focus_matches = text_matches + path_matches;
            let focus_boost = text_matches as f64 * 0.35 + path_matches as f64 * 0.7;
            let score = scores[&file.path] + focus_boost;
            FileRank { path: file.path.clone(), score: scaled_score(score), focus_matches }
        })
        .collect::<Vec<_>>();
    ranking.sort_by(|left, right| right.score.cmp(&left.score).then_with(|| left.path.cmp(&right.path)));
    ranking
}

pub fn select_snippets(
    files: &[SourceFile], edges: &[LexicalEdge], ranking: &[FileRank], token_budget: usize, settings: &MapSettings,
) -> MapSelection {
    let mut reference_counts = BTreeMap::<(String, String), u64>::new();
    for edge in edges {
        *reference_counts
            .entry((edge.target.clone(), edge.symbol.clone()))
            .or_default() += 1;
    }
    let file_scores = ranking
        .iter()
        .map(|rank| (rank.path.as_str(), rank.score))
        .collect::<BTreeMap<_, _>>();
    let mut candidates = Vec::new();
    for file in files {
        let file_score = *file_scores.get(file.path.as_str()).unwrap_or(&0);
        for symbol in file
            .symbols
            .iter()
            .filter(|symbol| symbol.role == SymbolRole::Definition)
        {
            let reference_count = reference_counts
                .get(&(file.path.clone(), symbol.name.clone()))
                .copied()
                .unwrap_or_default();
            let focus_boost = settings
                .focuses
                .iter()
                .filter(|focus| symbol_matches_focus(symbol, focus))
                .count() as u64
                * 250_000;
            let symbol_score = file_score
                .saturating_add(reference_count.saturating_mul(1_000))
                .saturating_add(focus_boost);
            candidates.push(SnippetCandidate {
                path: file.path.clone(),
                language: file.language,
                symbol: symbol.clone(),
                score: symbol_score,
            });
        }
    }
    candidates.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| location_key(Some(&left.symbol.location)).cmp(&location_key(Some(&right.symbol.location))))
            .then_with(|| left.symbol.name.cmp(&right.symbol.name))
    });

    let mut snippets = Vec::new();
    let mut estimated_tokens = 0;
    for candidate in candidates {
        let remaining = token_budget.saturating_sub(estimated_tokens);
        let Some((symbol, cost, truncated)) = fit_snippet(&candidate, remaining) else {
            continue;
        };
        estimated_tokens += cost;
        snippets.push(MapSnippet {
            path: candidate.path,
            language: candidate.language,
            symbol,
            score: candidate.score,
            estimated_tokens: cost,
            truncated,
        });
        if estimated_tokens >= token_budget {
            break;
        }
    }

    MapSelection { token_budget, estimated_tokens, snippets }
}

fn is_graph_definition(symbol: &SourceSymbol) -> bool {
    symbol.role == SymbolRole::Definition
        && matches!(
            symbol.kind,
            SymbolKind::Function
                | SymbolKind::Struct
                | SymbolKind::Enum
                | SymbolKind::Trait
                | SymbolKind::Type
                | SymbolKind::Const
                | SymbolKind::Static
                | SymbolKind::Module
                | SymbolKind::Macro
                | SymbolKind::Class
                | SymbolKind::Method
                | SymbolKind::Interface
        )
}

fn is_graph_reference(symbol: &SourceSymbol) -> bool {
    symbol.role == SymbolRole::Reference
        && !matches!(
            symbol.evidence,
            SymbolEvidence::BareReference | SymbolEvidence::MemberReference
        )
        && symbol.kind != SymbolKind::Field
        && !is_generic_name(&symbol.name)
}

fn import_module_hints(context: &str, language: SourceLanguage) -> Vec<String> {
    let mut hints = Vec::new();
    let mut quoted = None;
    for quote in ['"', '\''] {
        if let Some(start) = context.find(quote)
            && let Some(end) = context[start + 1..].find(quote)
        {
            quoted = Some(context[start + 1..start + 1 + end].to_owned());
            break;
        }
    }
    if let Some(value) = quoted {
        let normalized = normalize_module_hint(&value);
        hints.push(normalized.clone());
        if language == SourceLanguage::Lua && normalized.contains('.') {
            hints.push(normalized.replace('.', "/"));
        }
    }
    let words = context.split_whitespace().collect::<Vec<_>>();
    if let Some(index) = words.iter().position(|word| *word == "from")
        && let Some(module) = words.get(index + 1)
    {
        hints.push(normalize_module_hint(module));
    }
    hints.extend(
        context
            .split(|character: char| character.is_whitespace() || matches!(character, ';' | ',' | '(' | ')'))
            .filter(|part| part.contains("::") || part.contains('/'))
            .map(normalize_module_hint),
    );
    hints.retain(|hint| !hint.is_empty());
    hints.sort();
    hints.dedup();
    hints
}

fn normalize_module_hint(value: &str) -> String {
    let value = value.trim_matches(['"', '\'', '`', ';', ',']);
    let value = value.trim_start_matches("./").trim_start_matches("../");
    value
        .replace('\\', "/")
        .trim_end_matches("/__init__")
        .trim_end_matches("/mod")
        .trim_end_matches(".js")
        .trim_end_matches(".ts")
        .trim_end_matches(".tsx")
        .trim_end_matches(".py")
        .trim_end_matches(".rb")
        .trim_end_matches(".rs")
        .trim_end_matches(".java")
        .trim_end_matches(".cs")
        .trim_end_matches(".go")
        .trim_end_matches(".lua")
        .trim_end_matches(".rockspec")
        .replace("::", "/")
        .trim_matches('/')
        .to_ascii_lowercase()
}

fn module_path_matches(path: &str, hints: &[String]) -> bool {
    if hints.is_empty() {
        return false;
    }
    let normalized = normalize_module_hint(path);
    hints.iter().any(|hint| {
        let direct_match = normalized == *hint || normalized.ends_with(&format!("/{hint}"));
        let module = hint
            .rsplit_once('/')
            .map(|(module, _)| module)
            .unwrap_or(hint)
            .trim_start_matches("crate/")
            .trim_start_matches("self/")
            .trim_start_matches("super/");
        let imported_directory = hint.rsplit('/').next().unwrap_or(hint);
        let path_parent = repository_parent(&normalized);
        direct_match
            || normalized == module
            || normalized.ends_with(&format!("/{module}"))
            || path_parent == imported_directory
            || path_parent.ends_with(&format!("/{imported_directory}"))
    })
}

fn symbol_matches_focus(symbol: &SourceSymbol, focus: &str) -> bool {
    let focus = focus.trim().to_ascii_lowercase();
    !focus.is_empty()
        && (symbol.name.to_ascii_lowercase().contains(&focus) || symbol.context.to_ascii_lowercase().contains(&focus))
}

fn fit_snippet(candidate: &SnippetCandidate, budget: usize) -> Option<(SourceSymbol, usize, bool)> {
    let scope = if candidate.symbol.scope.is_empty() {
        "root".to_owned()
    } else {
        candidate.symbol.scope.join("::")
    };
    let prefix = format!(
        "{} {} {} {}:{}-{}:{} {}",
        candidate.path,
        candidate.symbol.kind.label(),
        candidate.symbol.name,
        candidate.symbol.location.start.line,
        candidate.symbol.location.start.column,
        candidate.symbol.location.end.line,
        candidate.symbol.location.end.column,
        scope
    );
    let full = format!("{prefix} {}", candidate.symbol.context);
    let full_cost = utils::token_count(&full);
    if full_cost <= budget {
        return Some((candidate.symbol.clone(), full_cost, false));
    }
    let marker = "…";
    if utils::token_count(&format!("{prefix} {marker}")) > budget {
        return None;
    }
    let max_chars = candidate.symbol.context.chars().count();
    let mut best = 0;
    for chars in 0..=max_chars {
        let context = candidate.symbol.context.chars().take(chars).collect::<String>();
        if utils::token_count(&format!("{prefix} {context}{marker}")) <= budget {
            best = chars;
        } else {
            break;
        }
    }
    let context = candidate.symbol.context.chars().take(best).collect::<String>();
    let mut symbol = candidate.symbol.clone();
    symbol.context = format!("{context}{marker}");
    let cost = utils::token_count(&format!("{prefix} {}", symbol.context));
    Some((symbol, cost, true))
}

fn lexical_weight(symbol: &str) -> f64 {
    if is_generic_name(symbol) || symbol.starts_with('_') { 0.25 } else { 1.0 }
}

fn edge_weight(edge: &LexicalEdge) -> f64 {
    let confidence = match edge.confidence {
        ConfidenceTier::High => 1.0,
        ConfidenceTier::Medium => 0.5,
        ConfidenceTier::Low => 0.25,
    };
    let visibility = match edge.target_visibility {
        SymbolVisibility::Public => 1.0,
        SymbolVisibility::Internal => 0.8,
        SymbolVisibility::Private => 0.35,
        SymbolVisibility::Unknown => 0.7,
    };
    lexical_weight(&edge.symbol) * confidence * visibility
}

fn is_generic_name(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "data" | "default" | "error" | "item" | "key" | "main" | "new" | "result" | "self" | "value"
    )
}

fn file_matches_focus(file: &SourceFile, focus: &str) -> bool {
    let focus = focus.trim().to_ascii_lowercase();
    !focus.is_empty()
        && (file.path.to_ascii_lowercase().contains(&focus)
            || file.symbols.iter().any(|symbol| {
                symbol.name.to_ascii_lowercase().contains(&focus)
                    || symbol.context.to_ascii_lowercase().contains(&focus)
            }))
}

fn path_matches_focus(path: &str, focus_path: &str) -> bool {
    let focus_path = focus_path.trim().replace('\\', "/");
    let focus_path = focus_path.trim_start_matches("./");
    !focus_path.is_empty() && (path == focus_path || path.starts_with(&format!("{focus_path}/")))
}

fn scaled_score(score: f64) -> u64 {
    (score.max(0.0) * RANK_SCALE).round() as u64
}

fn same_module_candidates(
    source: &SourceFile, candidates: &[(String, SymbolVisibility)], modules: &BTreeMap<String, BTreeSet<String>>,
) -> Option<Vec<(String, SymbolVisibility)>> {
    if source.language != SourceLanguage::Go {
        return None;
    }
    let source_modules = modules.get(&source.path)?;
    if source_modules.is_empty() {
        return None;
    }
    let source_parent = repository_parent(&source.path);
    let matches = candidates
        .iter()
        .filter(|(path, _)| {
            path != &source.path
                && repository_parent(path) == source_parent
                && modules
                    .get(path)
                    .is_some_and(|target_modules| !source_modules.is_disjoint(target_modules))
        })
        .cloned()
        .collect::<Vec<_>>();
    (!matches.is_empty()).then_some(matches)
}

fn repository_parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(parent, _)| parent)
}
