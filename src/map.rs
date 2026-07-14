use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use gix::bstr::ByteSlice;
use ignore::WalkBuilder;
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use tree_sitter::{Node, Parser, Query, QueryCursor, StreamingIterator};

use crate::cli::ExitCategory;
use crate::report::{
    FileAnalysisStatus, MapFinding, MapFindingKind, MapInventory, MapReport, OmissionReason, SourceFile,
    SourceLanguage, SourceLocation, SourceOmission, SourceSymbol, SymbolKind, SymbolRole, WorktreeState,
};

const MAX_CONTEXT_CHARS: usize = 180;

type LanguageFactory = fn() -> tree_sitter::Language;

const RUST_DECLARATION_KINDS: &[&str] = &[
    "function_item",
    "struct_item",
    "enum_item",
    "trait_item",
    "type_item",
    "const_item",
    "static_item",
    "mod_item",
    "macro_definition",
    "field_declaration",
];

const RUST_SCOPE_KINDS: &[&str] = &[
    "function_item",
    "struct_item",
    "enum_item",
    "trait_item",
    "type_item",
    "const_item",
    "static_item",
    "mod_item",
    "macro_definition",
    "impl_item",
];

const JAVASCRIPT_DECLARATION_KINDS: &[&str] = &[
    "function_declaration",
    "generator_function_declaration",
    "class_declaration",
    "method_definition",
    "variable_declarator",
    "import_specifier",
    "import_clause",
    "namespace_import",
    "named_imports",
];

const JAVASCRIPT_SCOPE_KINDS: &[&str] = &[
    "function_declaration",
    "generator_function_declaration",
    "function",
    "arrow_function",
    "class_declaration",
    "method_definition",
];

const TYPESCRIPT_DECLARATION_KINDS: &[&str] = &[
    "function_declaration",
    "generator_function_declaration",
    "class_declaration",
    "interface_declaration",
    "type_alias_declaration",
    "enum_declaration",
    "method_definition",
    "variable_declarator",
    "import_specifier",
    "import_clause",
    "namespace_import",
    "named_imports",
];

const TYPESCRIPT_SCOPE_KINDS: &[&str] = &[
    "function_declaration",
    "generator_function_declaration",
    "function",
    "arrow_function",
    "class_declaration",
    "interface_declaration",
    "enum_declaration",
    "method_definition",
];

#[derive(Clone, Copy, Debug)]
struct LanguageSupport {
    language: SourceLanguage,
    extensions: &'static [&'static str],
    query_pack: &'static str,
    grammar: LanguageFactory,
    definitions: &'static str,
    references: &'static str,
    declaration_kinds: &'static [&'static str],
    scope_kinds: &'static [&'static str],
}

const RUST_SUPPORT: LanguageSupport = LanguageSupport {
    language: SourceLanguage::Rust,
    extensions: &["rs"],
    query_pack: "rust-v1",
    grammar: rust_language,
    definitions: include_str!("queries/rust/definitions.scm"),
    references: include_str!("queries/rust/references.scm"),
    declaration_kinds: RUST_DECLARATION_KINDS,
    scope_kinds: RUST_SCOPE_KINDS,
};

const JAVASCRIPT_SUPPORT: LanguageSupport = LanguageSupport {
    language: SourceLanguage::JavaScript,
    extensions: &["js", "mjs", "cjs"],
    query_pack: "javascript-v1",
    grammar: javascript_language,
    definitions: include_str!("queries/javascript/definitions.scm"),
    references: include_str!("queries/javascript/references.scm"),
    declaration_kinds: JAVASCRIPT_DECLARATION_KINDS,
    scope_kinds: JAVASCRIPT_SCOPE_KINDS,
};

const JAVASCRIPT_JSX_SUPPORT: LanguageSupport = LanguageSupport {
    language: SourceLanguage::JavaScriptJsx,
    extensions: &["jsx"],
    query_pack: "javascript-v1",
    grammar: javascript_language,
    definitions: include_str!("queries/javascript/definitions.scm"),
    references: include_str!("queries/javascript/references.scm"),
    declaration_kinds: JAVASCRIPT_DECLARATION_KINDS,
    scope_kinds: JAVASCRIPT_SCOPE_KINDS,
};

const TYPESCRIPT_SUPPORT: LanguageSupport = LanguageSupport {
    language: SourceLanguage::TypeScript,
    extensions: &["ts", "mts", "cts"],
    query_pack: "typescript-v1",
    grammar: typescript_language,
    definitions: include_str!("queries/typescript/definitions.scm"),
    references: include_str!("queries/typescript/references.scm"),
    declaration_kinds: TYPESCRIPT_DECLARATION_KINDS,
    scope_kinds: TYPESCRIPT_SCOPE_KINDS,
};

const TYPESCRIPT_TSX_SUPPORT: LanguageSupport = LanguageSupport {
    language: SourceLanguage::TypeScriptTsx,
    extensions: &["tsx"],
    query_pack: "typescript-v1",
    grammar: tsx_language,
    definitions: include_str!("queries/typescript/definitions.scm"),
    references: include_str!("queries/typescript/references.scm"),
    declaration_kinds: TYPESCRIPT_DECLARATION_KINDS,
    scope_kinds: TYPESCRIPT_SCOPE_KINDS,
};

const LANGUAGE_SUPPORT: &[LanguageSupport] = &[
    RUST_SUPPORT,
    JAVASCRIPT_SUPPORT,
    JAVASCRIPT_JSX_SUPPORT,
    TYPESCRIPT_SUPPORT,
    TYPESCRIPT_TSX_SUPPORT,
];

type Result<T> = std::result::Result<T, MapError>;

#[derive(Debug, thiserror::Error)]
pub enum MapError {
    #[error("could not discover a Git repository from `{path}`: {source}")]
    Discovery {
        path: PathBuf,
        #[source]
        source: Box<gix::discover::Error>,
    },
    #[error("map input `{path}` is invalid: {reason}")]
    Input { path: PathBuf, reason: String },
    #[error("map analysis failed while {operation}: {reason}")]
    Analysis { operation: &'static str, reason: String },
}

impl From<MapError> for ExitCategory {
    fn from(error: MapError) -> Self {
        match error {
            MapError::Discovery { .. } => ExitCategory::Repository,
            MapError::Input { .. } => ExitCategory::Input,
            MapError::Analysis { .. } => ExitCategory::Analysis,
        }
    }
}

impl From<&MapError> for ExitCategory {
    fn from(error: &MapError) -> Self {
        match error {
            MapError::Discovery { .. } => ExitCategory::Repository,
            MapError::Input { .. } => ExitCategory::Input,
            MapError::Analysis { .. } => ExitCategory::Analysis,
        }
    }
}

impl MapError {
    fn analysis(operation: &'static str, error: impl std::fmt::Display) -> Self {
        Self::Analysis { operation, reason: error.to_string() }
    }
}

#[derive(Clone, Debug, Default)]
pub struct MapSettings {
    pub excludes: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Scope {
    relative_path: String,
}

#[derive(Clone, Debug)]
struct Candidate {
    state: WorktreeState,
    symlink: bool,
}

#[derive(Debug)]
struct ParsedSource {
    symbols: Vec<SourceSymbol>,
    findings: Vec<MapFinding>,
    status: FileAnalysisStatus,
    limitations: Vec<String>,
}

pub fn analyze(path: &Path, settings: &MapSettings) -> Result<MapReport> {
    let selected_path = absolute_path(path)?;
    let repository = gix::discover(&selected_path)
        .map_err(|source| MapError::Discovery { path: selected_path.clone(), source: Box::new(source) })?;
    let repository_root = repository.workdir().ok_or_else(|| MapError::Input {
        path: selected_path.clone(),
        reason: "the discovered repository has no worktree".to_owned(),
    })?;
    let repository_root = fs::canonicalize(repository_root)
        .map_err(|error| MapError::Input { path: repository_root.to_owned(), reason: error.to_string() })?;
    let relative_path = selected_path
        .strip_prefix(&repository_root)
        .map_err(|_| MapError::Input {
            path: selected_path.clone(),
            reason: format!("path is outside repository `{}`", repository_root.display()),
        })?;
    let relative_path = if relative_path.as_os_str().is_empty() {
        ".".to_owned()
    } else {
        relative_path.to_string_lossy().replace('\\', "/")
    };
    let scope = Scope { relative_path };

    let exclusions = build_exclusions(&repository_root, &settings.excludes)?;
    let mut tracked_paths = BTreeSet::new();
    collect_tree_files(
        &repository,
        &repository
            .head_tree_id_or_empty()
            .map_err(|error| MapError::analysis("resolving the repository HEAD tree", error))?,
        "",
        &mut tracked_paths,
    )?;
    collect_index_files(&repository, &mut tracked_paths)?;
    let modified_paths = collect_modified_paths(&repository)?;

    let mut candidates = BTreeMap::new();
    for path in tracked_paths
        .into_iter()
        .filter(|path| in_scope(path, &scope.relative_path))
    {
        let state = if modified_paths.contains(&path) { WorktreeState::Modified } else { WorktreeState::Tracked };
        candidates.insert(path, Candidate { state, symlink: false });
    }

    let (visible_paths, visible_errors) = walk_files(&selected_path, &repository_root, true);
    for (path, symlink) in &visible_paths {
        if is_git_internal(path) || !in_scope(path, &scope.relative_path) {
            continue;
        }
        candidates
            .entry(path.clone())
            .and_modify(|candidate| candidate.symlink |= *symlink)
            .or_insert(Candidate { state: WorktreeState::Untracked, symlink: *symlink });
    }

    let (all_paths, all_errors) = walk_files(&selected_path, &repository_root, false);
    let visible_path_set: BTreeSet<_> = visible_paths.keys().cloned().collect();
    let mut omissions = Vec::new();
    for error in visible_errors.into_iter().chain(all_errors) {
        omissions.push(SourceOmission {
            path: scope.relative_path.clone(),
            reason: OmissionReason::TraversalError,
            detail: error,
        });
    }
    for (path, symlink) in all_paths {
        if is_git_internal(&path)
            || !in_scope(&path, &scope.relative_path)
            || candidates.contains_key(&path)
            || visible_path_set.contains(&path)
            || support_for_path(Path::new(&path)).is_none()
        {
            continue;
        }
        let reason = if symlink { OmissionReason::Symlink } else { OmissionReason::IgnoredUntracked };
        let detail = if symlink {
            "Symlinked source paths are omitted so map traversal cannot follow a path outside the requested scope."
        } else {
            "The untracked Rust file was omitted by the ignore crate traversal policy."
        };
        omissions.push(SourceOmission { path, reason, detail: detail.to_owned() });
    }

    let inventory = inventory(&candidates);
    let mut files = Vec::new();
    let mut findings = Vec::new();
    for (path, candidate) in candidates {
        let absolute = repository_root.join(&path);
        if explicitly_excluded(exclusions.as_ref(), &absolute) {
            omissions.push(SourceOmission {
                path,
                reason: OmissionReason::ExplicitExclusion,
                detail: "The caller supplied an exclusion glob for this path.".to_owned(),
            });
            continue;
        }
        let Some(support) = support_for_path(Path::new(&path)) else {
            omissions.push(SourceOmission {
                path: path.clone(),
                reason: OmissionReason::UnsupportedLanguage,
                detail:
                    "The file extension is not registered with a first-class language parser; the path was not parsed."
                        .to_owned(),
            });
            continue;
        };
        let symlink = candidate.symlink
            || fs::symlink_metadata(&absolute)
                .map(|metadata| metadata.file_type().is_symlink())
                .unwrap_or(false);
        if symlink {
            omissions.push(SourceOmission {
                path,
                reason: OmissionReason::Symlink,
                detail: "Symlinked source paths are omitted so map traversal cannot follow a path outside the requested scope."
                    .to_owned(),
            });
            continue;
        }
        let source = match fs::read(&absolute) {
            Ok(source) => source,
            Err(error) => {
                omissions.push(SourceOmission {
                    path,
                    reason: OmissionReason::ReadError,
                    detail: format!("The source file could not be read: {error}"),
                });
                continue;
            }
        };
        let ParsedSource { symbols, findings: file_findings, status, limitations } = parse_source(&source, support);
        findings.extend(file_findings.into_iter().map(|mut finding| {
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
        });
    }

    add_ambiguity_findings(&files, &mut findings);
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
    let limitations = if has_non_rust_files {
        vec![
            "Definitions and references are extracted lexically with language-specific Tree-sitter queries; imports, types, macros, and runtime behavior are not resolved."
                .to_owned(),
            "Reference names can have multiple lexical definition candidates; ambiguity is reported rather than treated as a semantic call edge."
                .to_owned(),
            "JavaScript/JSX and TypeScript/TSX use explicit grammar variants; query-pack provenance is reported per variant."
                .to_owned(),
            "Tracked files are eligible even when ignore rules match them; ignored untracked files are omitted and recorded."
                .to_owned(),
        ]
    } else {
        vec![
            "Rust definitions and references are extracted lexically; imports, types, macros, and runtime behavior are not resolved."
                .to_owned(),
            "Reference names can have multiple lexical definition candidates; ambiguity is reported rather than treated as a semantic call edge."
                .to_owned(),
            "Tracked files are eligible even when ignore rules match them; ignored untracked files are omitted and recorded."
                .to_owned(),
        ]
    };

    Ok(MapReport {
        repository_root: repository_root.to_string_lossy().into_owned(),
        scope_path: scope.relative_path,
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
        files,
        omissions,
        findings,
        limitations,
    })
}

fn absolute_path(path: &Path) -> Result<PathBuf> {
    let path = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()
            .map_err(|error| MapError::analysis("reading the current directory", error))?
            .join(path)
    };
    fs::canonicalize(&path).map_err(|error| MapError::Input { path, reason: error.to_string() })
}

fn build_exclusions(root: &Path, exclusions: &[String]) -> Result<Option<Gitignore>> {
    if exclusions.is_empty() {
        return Ok(None);
    }
    let mut builder = GitignoreBuilder::new(root);
    for exclusion in exclusions {
        if exclusion.trim().is_empty() {
            return Err(MapError::Input {
                path: root.to_owned(),
                reason: "exclusion globs must not be empty".to_owned(),
            });
        }
        builder.add_line(None, exclusion).map_err(|error| MapError::Input {
            path: root.to_owned(),
            reason: format!("invalid exclusion glob `{exclusion}`: {error}"),
        })?;
    }
    builder.build().map(Some).map_err(|error| MapError::Input {
        path: root.to_owned(),
        reason: format!("could not compile exclusion globs: {error}"),
    })
}

fn explicitly_excluded(exclusions: Option<&Gitignore>, path: &Path) -> bool {
    exclusions.is_some_and(|matcher| matcher.matched_path_or_any_parents(path, false).is_ignore())
}

fn collect_tree_files(
    repository: &gix::Repository, tree_id: &gix::Id<'_>, prefix: &str, files: &mut BTreeSet<String>,
) -> Result<()> {
    let tree = repository
        .find_tree(*tree_id)
        .map_err(|error| MapError::analysis("reading the tracked source tree", error))?;
    for entry in tree.iter() {
        let entry = entry.map_err(|error| MapError::analysis("decoding a tracked tree entry", error))?;
        let name = entry.filename().to_str_lossy();
        let path = if prefix.is_empty() { name.into_owned() } else { format!("{prefix}/{name}") };
        if entry.mode().is_tree() {
            collect_tree_files(repository, &entry.id(), &path, files)?;
        } else {
            files.insert(path);
        }
    }
    Ok(())
}

fn collect_index_files(repository: &gix::Repository, files: &mut BTreeSet<String>) -> Result<()> {
    let index = repository
        .index_or_empty()
        .map_err(|error| MapError::analysis("reading the worktree index", error))?;
    for path in index.entries_with_paths_by_filter_map(|path, _| Some(path.to_str_lossy().into_owned())) {
        files.insert(path.1);
    }
    Ok(())
}

fn collect_modified_paths(repository: &gix::Repository) -> Result<BTreeSet<String>> {
    let index = repository
        .index_or_load_from_head_or_empty()
        .map_err(|error| MapError::analysis("loading the worktree index for status", error))?;
    let iterator = repository
        .status(gix::progress::Discard)
        .map_err(|error| MapError::analysis("starting worktree status", error))?
        .index(index)
        .index_worktree_submodules(None)
        .untracked_files(gix::status::UntrackedFiles::None)
        .into_iter(Vec::<gix::bstr::BString>::new())
        .map_err(|error| MapError::analysis("computing worktree status", error))?;
    let mut modified = BTreeSet::new();
    for item in iterator {
        let item = item.map_err(|error| MapError::analysis("reading worktree status", error))?;
        match item {
            gix::status::Item::IndexWorktree(gix::status::index_worktree::Item::Modification { rela_path, .. }) => {
                modified.insert(rela_path.to_str_lossy().into_owned());
            }
            gix::status::Item::TreeIndex(change) => {
                modified.insert(change.location().to_str_lossy().into_owned());
            }
            gix::status::Item::IndexWorktree(_) => {}
        }
    }
    Ok(modified)
}

fn walk_files(root: &Path, repository_root: &Path, standard_filters: bool) -> (BTreeMap<String, bool>, Vec<String>) {
    let mut builder = WalkBuilder::new(root);
    builder
        .standard_filters(standard_filters)
        .follow_links(false)
        .sort_by_file_path(|left, right| left.cmp(right));
    let mut files = BTreeMap::new();
    let mut errors = Vec::new();
    for item in builder.build() {
        let entry = match item {
            Ok(entry) => entry,
            Err(error) => {
                errors.push(format!("ignore traversal reported an error: {error}"));
                continue;
            }
        };
        let Some(file_type) = entry.file_type() else {
            continue;
        };
        if !(file_type.is_file() || entry.path_is_symlink()) {
            continue;
        }
        let Some(path) = entry.path().strip_prefix(repository_root).ok() else {
            continue;
        };
        let path = path.to_string_lossy().replace('\\', "/");
        files.insert(path, entry.path_is_symlink());
    }
    (files, errors)
}

fn in_scope(path: &str, scope: &str) -> bool {
    scope == "." || path == scope || path.starts_with(&format!("{scope}/"))
}

fn is_git_internal(path: &str) -> bool {
    path == ".git" || path.starts_with(".git/")
}

fn inventory(candidates: &BTreeMap<String, Candidate>) -> (usize, usize, usize) {
    let tracked = candidates
        .values()
        .filter(|candidate| !matches!(candidate.state, WorktreeState::Untracked))
        .count();
    let modified = candidates
        .values()
        .filter(|candidate| matches!(candidate.state, WorktreeState::Modified))
        .count();
    let untracked = candidates
        .values()
        .filter(|candidate| matches!(candidate.state, WorktreeState::Untracked))
        .count();
    (tracked, modified, untracked)
}

fn parse_source(source: &[u8], support: &LanguageSupport) -> ParsedSource {
    let language = (support.grammar)();
    let mut parser = Parser::new();
    let mut findings = Vec::new();
    if let Err(error) = parser.set_language(&language) {
        findings.push(MapFinding {
            kind: MapFindingKind::ParserError,
            path: String::new(),
            location: None,
            detail: format!(
                "Could not configure the {} parser: {error}.",
                support.language.display_label()
            ),
        });
        return ParsedSource {
            symbols: Vec::new(),
            findings,
            status: FileAnalysisStatus::Partial,
            limitations: vec![format!(
                "The {} parser could not be configured; no symbols were extracted.",
                support.language.display_label()
            )],
        };
    }
    let Some(tree) = parser.parse(source, None) else {
        findings.push(MapFinding {
            kind: MapFindingKind::ParseError,
            path: String::new(),
            location: None,
            detail: format!(
                "The {} parser did not return a syntax tree.",
                support.language.display_label()
            ),
        });
        return ParsedSource {
            symbols: Vec::new(),
            findings,
            status: FileAnalysisStatus::Partial,
            limitations: vec![format!(
                "The {} parser did not return a syntax tree; no symbols were extracted.",
                support.language.display_label()
            )],
        };
    };

    let mut symbols = Vec::new();
    let mut definition_nodes = BTreeSet::new();
    let mut cursor = QueryCursor::new();
    let mut query_failed = false;
    if let Some(definition_query) = compile_query(&language, support.definitions, support, "definition", &mut findings)
    {
        let mut matches = cursor.matches(&definition_query, tree.root_node(), source);
        while let Some(query_match) = matches.next() {
            for capture in query_match.captures {
                let node = capture.node;
                definition_nodes.insert(node.id());
                symbols.push(symbol_from_capture(
                    node,
                    capture_name(&definition_query, capture.index),
                    SymbolRole::Definition,
                    source,
                    support,
                ));
            }
        }
    } else {
        query_failed = true;
    }
    if let Some(reference_query) = compile_query(&language, support.references, support, "reference", &mut findings) {
        let mut matches = cursor.matches(&reference_query, tree.root_node(), source);
        while let Some(query_match) = matches.next() {
            for capture in query_match.captures {
                let node = capture.node;
                if definition_nodes.contains(&node.id()) {
                    continue;
                }
                symbols.push(symbol_from_capture(
                    node,
                    capture_name(&reference_query, capture.index),
                    SymbolRole::Reference,
                    source,
                    support,
                ));
            }
        }
    } else {
        query_failed = true;
    }

    symbols.sort_by(|left, right| {
        location_key(Some(&left.location))
            .cmp(&location_key(Some(&right.location)))
            .then_with(|| left.role.label().cmp(right.role.label()))
            .then_with(|| left.name.cmp(&right.name))
    });
    symbols.dedup_by(|right, left| {
        right.name == left.name && right.kind == left.kind && right.role == left.role && right.location == left.location
    });

    collect_parse_findings(tree.root_node(), source, &mut findings);
    let status = if tree.root_node().has_error() || query_failed {
        FileAnalysisStatus::Partial
    } else {
        FileAnalysisStatus::Complete
    };
    let mut limitations = Vec::new();
    if tree.root_node().has_error() {
        limitations.push(format!(
            "Tree-sitter reported parse errors in this {} file; extracted symbols may be incomplete.",
            support.language.display_label()
        ));
    }
    if query_failed {
        limitations.push(format!(
            "One or more {} query-pack queries failed; available query findings were retained.",
            support.language.display_label()
        ));
    }
    ParsedSource { symbols, findings, status, limitations }
}

fn compile_query(
    language: &tree_sitter::Language, source: &str, support: &LanguageSupport, role: &str,
    findings: &mut Vec<MapFinding>,
) -> Option<Query> {
    match Query::new(language, source) {
        Ok(query) => Some(query),
        Err(error) => {
            findings.push(MapFinding {
                kind: MapFindingKind::QueryError,
                path: String::new(),
                location: None,
                detail: format!(
                    "Could not compile the {} {} query in query pack `{}`: {error}.",
                    support.language.display_label(),
                    role,
                    support.query_pack
                ),
            });
            None
        }
    }
}

fn capture_name(query: &Query, index: u32) -> &str {
    query
        .capture_names()
        .get(index as usize)
        .copied()
        .unwrap_or("reference.identifier")
}

fn symbol_from_capture(
    node: Node<'_>, capture_name: &str, role: SymbolRole, source: &[u8], support: &LanguageSupport,
) -> SourceSymbol {
    let declaration = declaration_node(node, support.declaration_kinds);
    let scope_start = if role == SymbolRole::Definition { declaration.parent() } else { node.parent() };
    SourceSymbol {
        name: text_for_node(node, source),
        kind: symbol_kind(capture_name),
        role,
        scope: scope_for_node(scope_start, source, support.scope_kinds),
        location: SourceLocation::from(node),
        context: context_snippet(node, source, support.declaration_kinds),
    }
}

fn symbol_kind(capture_name: &str) -> SymbolKind {
    let kind = capture_name.rsplit('.').next().unwrap_or("identifier");
    match kind {
        "function" => SymbolKind::Function,
        "struct" => SymbolKind::Struct,
        "enum" => SymbolKind::Enum,
        "trait" => SymbolKind::Trait,
        "type" => SymbolKind::Type,
        "const" => SymbolKind::Const,
        "static" => SymbolKind::Static,
        "module" => SymbolKind::Module,
        "macro" => SymbolKind::Macro,
        "field" => SymbolKind::Field,
        "class" => SymbolKind::Class,
        "method" => SymbolKind::Method,
        "variable" => SymbolKind::Variable,
        "interface" => SymbolKind::Interface,
        "import" => SymbolKind::Import,
        "export" => SymbolKind::Export,
        "identifier" => SymbolKind::Identifier,
        _ => SymbolKind::Other,
    }
}

fn declaration_node<'a>(node: Node<'a>, declaration_kinds: &[&str]) -> Node<'a> {
    let mut current = node;
    while let Some(parent) = current.parent() {
        if declaration_kinds.contains(&parent.kind()) {
            return parent;
        }
        current = parent;
    }
    node
}

fn scope_for_node(start: Option<Node<'_>>, source: &[u8], scope_kinds: &[&str]) -> Vec<String> {
    let mut scopes = Vec::new();
    let mut current = start;
    while let Some(node) = current {
        if scope_kinds.contains(&node.kind())
            && let Some(name) = node.child_by_field_name("name")
        {
            scopes.push(text_for_node(name, source));
        }
        current = node.parent();
    }
    scopes.reverse();
    scopes
}

fn context_snippet(node: Node<'_>, source: &[u8], declaration_kinds: &[&str]) -> String {
    let declaration = declaration_node(node, declaration_kinds);
    let (start, end) = if declaration_kinds.contains(&declaration.kind()) {
        let end = declaration
            .child_by_field_name("body")
            .map(|body| body.start_byte())
            .unwrap_or_else(|| declaration.end_byte());
        (declaration.start_byte(), end)
    } else {
        let line_start = source[..node.start_byte().min(source.len())]
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map(|position| position + 1)
            .unwrap_or(0);
        let line_end = source[node.end_byte().min(source.len())..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map(|offset| node.end_byte().min(source.len()) + offset)
            .unwrap_or(source.len());
        (line_start, line_end)
    };
    let bytes = source
        .get(start.min(source.len())..end.min(source.len()))
        .unwrap_or_default();
    compact_text(bytes)
}

fn compact_text(bytes: &[u8]) -> String {
    let normalized = String::from_utf8_lossy(bytes)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let mut output = String::new();
    for (index, character) in normalized.chars().enumerate() {
        if index >= MAX_CONTEXT_CHARS {
            output.push('…');
            break;
        }
        output.push(character);
    }
    output
}

fn text_for_node(node: Node<'_>, source: &[u8]) -> String {
    source
        .get(node.start_byte().min(source.len())..node.end_byte().min(source.len()))
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
        .unwrap_or_default()
}

fn collect_parse_findings(node: Node<'_>, source: &[u8], findings: &mut Vec<MapFinding>) {
    if node.is_error() || node.is_missing() {
        findings.push(MapFinding {
            kind: MapFindingKind::ParseError,
            path: String::new(),
            location: Some(SourceLocation::from(node)),
            detail: format!(
                "Tree-sitter recovered from a {} node near `{}`.",
                node.kind(),
                context_snippet(node, source, &[])
            ),
        });
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_parse_findings(child, source, findings);
    }
}

fn add_ambiguity_findings(files: &[SourceFile], findings: &mut Vec<MapFinding>) {
    let mut definitions = BTreeMap::<String, Vec<String>>::new();
    for file in files {
        for symbol in &file.symbols {
            if symbol.role == SymbolRole::Definition {
                definitions
                    .entry(symbol.name.clone())
                    .or_default()
                    .push(file.path.clone());
            }
        }
    }
    for file in files {
        for symbol in &file.symbols {
            if symbol.role != SymbolRole::Reference {
                continue;
            }
            let Some(candidates) = definitions.get(&symbol.name) else {
                continue;
            };
            if candidates.len() > 1 {
                findings.push(MapFinding {
                    kind: MapFindingKind::AmbiguousReference,
                    path: file.path.clone(),
                    location: Some(symbol.location.clone()),
                    detail: format!(
                        "Lexical reference `{}` has {} definition candidates; no type-resolved relationship is asserted.",
                        symbol.name,
                        candidates.len()
                    ),
                });
            }
        }
    }
}

fn location_key(location: Option<&SourceLocation>) -> (usize, usize, usize, usize) {
    location.map_or((0, 0, 0, 0), |location| {
        (
            location.start.line,
            location.start.column,
            location.end.line,
            location.end.column,
        )
    })
}

fn rust_language() -> tree_sitter::Language {
    tree_sitter_rust::LANGUAGE.into()
}

fn javascript_language() -> tree_sitter::Language {
    tree_sitter_javascript::LANGUAGE.into()
}

fn typescript_language() -> tree_sitter::Language {
    tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
}

fn tsx_language() -> tree_sitter::Language {
    tree_sitter_typescript::LANGUAGE_TSX.into()
}

fn support_for_path(path: &Path) -> Option<&'static LanguageSupport> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    LANGUAGE_SUPPORT
        .iter()
        .find(|support| support.extensions.contains(&extension.as_str()))
}

fn extension_for_path(path: &Path) -> String {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map_or_else(String::new, |extension| extension.to_ascii_lowercase())
}

fn supported_query_packs(files: &[SourceFile]) -> BTreeMap<String, String> {
    let mut query_packs = BTreeMap::new();
    for file in files {
        if let Some(support) = support_for_path(Path::new(&file.path)) {
            query_packs.insert(support.language.label().to_owned(), support.query_pack.to_owned());
        }
    }
    if query_packs.is_empty() {
        query_packs.insert(
            RUST_SUPPORT.language.label().to_owned(),
            RUST_SUPPORT.query_pack.to_owned(),
        );
    }
    query_packs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_and_snippet_are_compact_and_one_based() {
        let source = b"mod outer { fn parse(value: usize) { let _ = value; } }";
        let ParsedSource { symbols, findings, status, limitations } = parse_source(source, &RUST_SUPPORT);

        assert!(findings.is_empty());
        assert_eq!(status, FileAnalysisStatus::Complete);
        assert!(limitations.is_empty());
        let parse = symbols
            .iter()
            .find(|symbol| symbol.name == "parse" && symbol.role == SymbolRole::Definition)
            .expect("function definition");
        assert_eq!(parse.location.start.line, 1);
        assert_eq!(parse.location.start.column, 16);
        assert_eq!(parse.scope, vec!["outer"]);
        assert!(parse.context.starts_with("fn parse(value: usize)"));
    }

    #[test]
    fn malformed_rust_is_partial_with_an_explicit_parse_finding() {
        let ParsedSource { symbols, findings, status, limitations } = parse_source(b"fn broken( {", &RUST_SUPPORT);

        assert_eq!(status, FileAnalysisStatus::Partial);
        assert!(!symbols.is_empty());
        assert!(!findings.is_empty());
        assert!(limitations.iter().any(|limitation| limitation.contains("parse errors")));
    }

    #[test]
    fn javascript_query_pack_extracts_module_class_and_function_symbols() {
        let source = br#"
            import { helper } from "./helper.js";
            export function build(value) { return new Widget(value, helper); }
            export class Widget { render() { return helper(); } }
        "#;
        let ParsedSource { symbols, findings, status, limitations } = parse_source(source, &JAVASCRIPT_SUPPORT);

        assert_eq!(status, FileAnalysisStatus::Complete, "{findings:?}");
        assert!(limitations.is_empty());
        assert!(
            findings
                .iter()
                .all(|finding| finding.kind != MapFindingKind::QueryError)
        );
        assert!(symbols.iter().any(|symbol| {
            symbol.name == "build" && symbol.kind == SymbolKind::Function && symbol.role == SymbolRole::Definition
        }));
        assert!(symbols.iter().any(|symbol| {
            symbol.name == "Widget" && symbol.kind == SymbolKind::Class && symbol.role == SymbolRole::Definition
        }));
        assert!(symbols.iter().any(|symbol| {
            symbol.name == "render" && symbol.kind == SymbolKind::Method && symbol.role == SymbolRole::Definition
        }));
        let render = symbols
            .iter()
            .find(|symbol| symbol.name == "render" && symbol.role == SymbolRole::Definition)
            .expect("method definition");
        assert_eq!(render.scope, vec!["Widget"]);
        assert!(render.location.start.line > 0);
        assert!(render.context.starts_with("render()"));
        assert!(
            symbols
                .iter()
                .any(|symbol| symbol.name == "helper" && symbol.role == SymbolRole::Reference)
        );
    }

    #[test]
    fn typescript_and_tsx_query_packs_extract_typed_symbols_without_cross_language_loss() {
        let typescript = br#"
            import { helper } from "./helper";
            export interface User { name: string; }
            export class Service { run(user: User) { return helper(user.name); } }
            export function create(user: User): Service { return new Service(); }
        "#;
        let tsx = br#"
            export function View(props: { label: string }) { return <button>{props.label}</button>; }
        "#;

        let parsed_typescript = parse_source(typescript, &TYPESCRIPT_SUPPORT);
        let parsed_tsx = parse_source(tsx, &TYPESCRIPT_TSX_SUPPORT);
        assert_eq!(
            parsed_typescript.status,
            FileAnalysisStatus::Complete,
            "{parsed_typescript:?}"
        );
        assert_eq!(parsed_tsx.status, FileAnalysisStatus::Complete, "{parsed_tsx:?}");
        assert!(
            parsed_typescript
                .findings
                .iter()
                .all(|finding| finding.kind != MapFindingKind::QueryError)
        );
        assert!(
            parsed_tsx
                .findings
                .iter()
                .all(|finding| finding.kind != MapFindingKind::QueryError)
        );
        assert!(
            parsed_typescript
                .symbols
                .iter()
                .any(|symbol| symbol.name == "User" && symbol.kind == SymbolKind::Interface)
        );
        assert!(
            parsed_typescript
                .symbols
                .iter()
                .any(|symbol| symbol.name == "Service" && symbol.kind == SymbolKind::Class)
        );
        assert!(
            parsed_typescript
                .symbols
                .iter()
                .any(|symbol| symbol.name == "helper" && symbol.role == SymbolRole::Reference)
        );
        let run = parsed_typescript
            .symbols
            .iter()
            .find(|symbol| symbol.name == "run" && symbol.role == SymbolRole::Definition)
            .expect("method definition");
        assert_eq!(run.scope, vec!["Service"]);
        assert!(run.context.starts_with("run(user: User)"));
        let view = parsed_tsx
            .symbols
            .iter()
            .find(|symbol| symbol.name == "View" && symbol.kind == SymbolKind::Function)
            .expect("TSX function definition");
        assert!(view.location.start.line > 0);
        assert!(view.context.starts_with("function View"));
    }

    #[test]
    fn javascript_typescript_and_jsx_extensions_select_explicit_language_variants() {
        assert_eq!(
            support_for_path(Path::new("module.js")).unwrap().language,
            SourceLanguage::JavaScript
        );
        assert_eq!(
            support_for_path(Path::new("module.jsx")).unwrap().language,
            SourceLanguage::JavaScriptJsx
        );
        assert_eq!(
            support_for_path(Path::new("module.ts")).unwrap().language,
            SourceLanguage::TypeScript
        );
        assert_eq!(
            support_for_path(Path::new("module.tsx")).unwrap().language,
            SourceLanguage::TypeScriptTsx
        );
        assert!(support_for_path(Path::new("module.vue")).is_none());
    }

    #[test]
    fn query_failure_is_partial_and_does_not_discard_definition_findings() {
        let support =
            LanguageSupport { references: "(not_a_real_javascript_node) @reference.identifier", ..JAVASCRIPT_SUPPORT };
        let parsed = parse_source(b"function build() {}", &support);

        assert_eq!(parsed.status, FileAnalysisStatus::Partial);
        assert!(
            parsed
                .findings
                .iter()
                .any(|finding| finding.kind == MapFindingKind::QueryError)
        );
        assert!(
            parsed
                .symbols
                .iter()
                .any(|symbol| symbol.name == "build" && symbol.role == SymbolRole::Definition)
        );
        assert!(
            parsed
                .limitations
                .iter()
                .any(|limitation| limitation.contains("query-pack queries failed"))
        );
    }
}
