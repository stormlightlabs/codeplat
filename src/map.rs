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
    SourceLocation, SourceOmission, SourceSymbol, SymbolKind, SymbolRole, WorktreeState,
};

const QUERY_PACK_VERSION: &str = "rust-v1";
const MAX_CONTEXT_CHARS: usize = 180;

const DEFINITION_QUERY: &str = include_str!("queries/rust/definitions.scm");
const REFERENCE_QUERY: &str = include_str!("queries/rust/references.scm");

const DECLARATION_KINDS: &[&str] = &[
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

const SCOPE_KINDS: &[&str] = &[
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
            || !is_rust_path(Path::new(&path))
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
        if !is_rust_path(Path::new(&path)) {
            omissions.push(SourceOmission {
                path: path.clone(),
                reason: OmissionReason::UnsupportedLanguage,
                detail: "Only Rust source files are first-class in this ticket; the path was not parsed.".to_owned(),
            });
            continue;
        }
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
        let ParsedSource { symbols, findings: file_findings, status, limitations } = parse_source(&source)?;
        findings.extend(file_findings.into_iter().map(|mut finding| {
            if finding.path.is_empty() {
                finding.path = path.clone();
            }
            finding
        }));
        files.push(SourceFile {
            path,
            language: "rust".to_owned(),
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

    Ok(MapReport {
        repository_root: repository_root.to_string_lossy().into_owned(),
        scope_path: scope.relative_path,
        query_pack: QUERY_PACK_VERSION.to_owned(),
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
        limitations: vec![
            "Rust definitions and references are extracted lexically; imports, types, macros, and runtime behavior are not resolved."
                .to_owned(),
            "Reference names can have multiple lexical definition candidates; ambiguity is reported rather than treated as a semantic call edge."
                .to_owned(),
            "Tracked files are eligible even when ignore rules match them; ignored untracked files are omitted and recorded."
                .to_owned(),
        ],
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

fn is_rust_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("rs"))
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

fn parse_source(source: &[u8]) -> Result<ParsedSource> {
    let language: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
    let mut parser = Parser::new();
    parser
        .set_language(&language)
        .map_err(|error| MapError::analysis("configuring the Rust Tree-sitter parser", error))?;
    let tree = parser.parse(source, None).ok_or_else(|| MapError::Analysis {
        operation: "parsing a Rust source file",
        reason: "Tree-sitter did not return a syntax tree".to_owned(),
    })?;
    let definition_query = Query::new(&language, DEFINITION_QUERY)
        .map_err(|error| MapError::analysis("compiling the Rust definition query pack", error))?;
    let reference_query = Query::new(&language, REFERENCE_QUERY)
        .map_err(|error| MapError::analysis("compiling the Rust reference query pack", error))?;

    let mut symbols = Vec::new();
    let mut definition_nodes = BTreeSet::new();
    let mut cursor = QueryCursor::new();
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
            ));
        }
    }
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
            ));
        }
    }

    symbols.sort_by(|left, right| {
        location_key(Some(&left.location))
            .cmp(&location_key(Some(&right.location)))
            .then_with(|| left.role.label().cmp(right.role.label()))
            .then_with(|| left.name.cmp(&right.name))
    });

    let mut findings = Vec::new();
    collect_parse_findings(tree.root_node(), source, &mut findings);
    let status = if tree.root_node().has_error() {
        findings.shrink_to_fit();
        FileAnalysisStatus::Partial
    } else {
        FileAnalysisStatus::Complete
    };
    let limitations = if status == FileAnalysisStatus::Partial {
        vec!["Tree-sitter reported parse errors; extracted symbols may be incomplete.".to_owned()]
    } else {
        Vec::new()
    };
    Ok(ParsedSource { symbols, findings, status, limitations })
}

fn capture_name(query: &Query, index: u32) -> &str {
    query
        .capture_names()
        .get(index as usize)
        .copied()
        .unwrap_or("reference.identifier")
}

fn symbol_from_capture(node: Node<'_>, capture_name: &str, role: SymbolRole, source: &[u8]) -> SourceSymbol {
    let declaration = declaration_node(node);
    let scope_start = if role == SymbolRole::Definition { declaration.parent() } else { node.parent() };
    SourceSymbol {
        name: text_for_node(node, source),
        kind: symbol_kind(capture_name),
        role,
        scope: scope_for_node(scope_start, source),
        location: SourceLocation::from(node),
        context: context_snippet(node, source),
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
        "identifier" => SymbolKind::Identifier,
        _ => SymbolKind::Other,
    }
}

fn declaration_node(node: Node<'_>) -> Node<'_> {
    let mut current = node;
    while let Some(parent) = current.parent() {
        if DECLARATION_KINDS.contains(&parent.kind()) {
            return parent;
        }
        current = parent;
    }
    node
}

fn scope_for_node(start: Option<Node<'_>>, source: &[u8]) -> Vec<String> {
    let mut scopes = Vec::new();
    let mut current = start;
    while let Some(node) = current {
        if SCOPE_KINDS.contains(&node.kind())
            && let Some(name) = node.child_by_field_name("name")
        {
            scopes.push(text_for_node(name, source));
        }
        current = node.parent();
    }
    scopes.reverse();
    scopes
}

fn context_snippet(node: Node<'_>, source: &[u8]) -> String {
    let declaration = declaration_node(node);
    let (start, end) = if DECLARATION_KINDS.contains(&declaration.kind()) {
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
                context_snippet(node, source)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_and_snippet_are_compact_and_one_based() {
        let source = b"mod outer { fn parse(value: usize) { let _ = value; } }";
        let ParsedSource { symbols, findings, status, limitations } = parse_source(source).expect("Rust parses");

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
        let ParsedSource { symbols, findings, status, limitations } =
            parse_source(b"fn broken( {").expect("parser recovers");

        assert_eq!(status, FileAnalysisStatus::Partial);
        assert!(!symbols.is_empty());
        assert!(!findings.is_empty());
        assert!(limitations.iter().any(|limitation| limitation.contains("parse errors")));
    }
}
