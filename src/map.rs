use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use gix::bstr::ByteSlice;
use ignore::WalkBuilder;
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use serde::{Deserialize, Serialize};
use tree_sitter::{Node, Parser, Query, QueryCursor, StreamingIterator};

use crate::cli::ExitCategory;
use crate::report::{
    CacheMode, CacheStatus, FileAnalysisStatus, FileRank, LexicalEdge, MapCacheReport, MapFinding, MapFindingKind,
    MapInventory, MapReport, MapSelection, MapSnippet, OmissionReason, SourceFile, SourceLanguage, SourceLocation,
    SourceOmission, SourceSymbol, SymbolKind, SymbolRole, WorktreeState,
};
use crate::security;

const MAX_CONTEXT_CHARS: usize = 180;
const DEFAULT_MAP_TOKENS: usize = 1_000;
const CACHE_SCHEMA_VERSION: u16 = 1;
const CACHE_TOOL_VERSION: &str = "setaryb-map-v7";
const RANK_SCALE: f64 = 1_000_000.0;
const PAGE_RANK_DAMPING: f64 = 0.85;
const PAGE_RANK_ITERATIONS: usize = 24;

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

const PYTHON_DECLARATION_KINDS: &[&str] = &[
    "function_definition",
    "class_definition",
    "assignment",
    "import_statement",
    "import_from_statement",
];

const PYTHON_SCOPE_KINDS: &[&str] = &["function_definition", "class_definition"];

const RUBY_DECLARATION_KINDS: &[&str] = &["method", "singleton_method", "class", "module", "assignment"];

const RUBY_SCOPE_KINDS: &[&str] = &["method", "singleton_method", "class", "module"];

const JAVA_DECLARATION_KINDS: &[&str] = &[
    "package_declaration",
    "import_declaration",
    "class_declaration",
    "record_declaration",
    "interface_declaration",
    "enum_declaration",
    "annotation_type_declaration",
    "method_declaration",
    "field_declaration",
];

const JAVA_SCOPE_KINDS: &[&str] = &[
    "package_declaration",
    "class_declaration",
    "record_declaration",
    "interface_declaration",
    "enum_declaration",
    "annotation_type_declaration",
    "method_declaration",
];

const C_SHARP_DECLARATION_KINDS: &[&str] = &[
    "namespace_declaration",
    "file_scoped_namespace_declaration",
    "class_declaration",
    "struct_declaration",
    "enum_declaration",
    "interface_declaration",
    "record_declaration",
    "method_declaration",
    "property_declaration",
    "field_declaration",
];

const C_SHARP_SCOPE_KINDS: &[&str] = &[
    "namespace_declaration",
    "file_scoped_namespace_declaration",
    "class_declaration",
    "struct_declaration",
    "enum_declaration",
    "interface_declaration",
    "record_declaration",
    "method_declaration",
    "property_declaration",
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

const PYTHON_SUPPORT: LanguageSupport = LanguageSupport {
    language: SourceLanguage::Python,
    extensions: &["py", "pyi"],
    query_pack: "python-v1",
    grammar: python_language,
    definitions: include_str!("queries/python/definitions.scm"),
    references: include_str!("queries/python/references.scm"),
    declaration_kinds: PYTHON_DECLARATION_KINDS,
    scope_kinds: PYTHON_SCOPE_KINDS,
};

const RUBY_SUPPORT: LanguageSupport = LanguageSupport {
    language: SourceLanguage::Ruby,
    extensions: &["rb", "rake", "gemspec"],
    query_pack: "ruby-v1",
    grammar: ruby_language,
    definitions: include_str!("queries/ruby/definitions.scm"),
    references: include_str!("queries/ruby/references.scm"),
    declaration_kinds: RUBY_DECLARATION_KINDS,
    scope_kinds: RUBY_SCOPE_KINDS,
};

const JAVA_SUPPORT: LanguageSupport = LanguageSupport {
    language: SourceLanguage::Java,
    extensions: &["java"],
    query_pack: "java-v1",
    grammar: java_language,
    definitions: include_str!("queries/java/definitions.scm"),
    references: include_str!("queries/java/references.scm"),
    declaration_kinds: JAVA_DECLARATION_KINDS,
    scope_kinds: JAVA_SCOPE_KINDS,
};

const C_SHARP_SUPPORT: LanguageSupport = LanguageSupport {
    language: SourceLanguage::CSharp,
    extensions: &["cs"],
    query_pack: "c-sharp-v1",
    grammar: c_sharp_language,
    definitions: include_str!("queries/c_sharp/definitions.scm"),
    references: include_str!("queries/c_sharp/references.scm"),
    declaration_kinds: C_SHARP_DECLARATION_KINDS,
    scope_kinds: C_SHARP_SCOPE_KINDS,
};

const LANGUAGE_SUPPORT: &[LanguageSupport] = &[
    RUST_SUPPORT,
    JAVASCRIPT_SUPPORT,
    JAVASCRIPT_JSX_SUPPORT,
    TYPESCRIPT_SUPPORT,
    TYPESCRIPT_TSX_SUPPORT,
    PYTHON_SUPPORT,
    RUBY_SUPPORT,
    JAVA_SUPPORT,
    C_SHARP_SUPPORT,
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
    #[error("map safety check failed while {operation}: {reason}")]
    Safety { operation: &'static str, reason: String },
}

impl From<MapError> for ExitCategory {
    fn from(error: MapError) -> Self {
        match error {
            MapError::Discovery { .. } => ExitCategory::Repository,
            MapError::Input { .. } => ExitCategory::Input,
            MapError::Analysis { .. } => ExitCategory::Analysis,
            MapError::Safety { .. } => ExitCategory::Input,
        }
    }
}

impl From<&MapError> for ExitCategory {
    fn from(error: &MapError) -> Self {
        match error {
            MapError::Discovery { .. } => ExitCategory::Repository,
            MapError::Input { .. } => ExitCategory::Input,
            MapError::Analysis { .. } => ExitCategory::Analysis,
            MapError::Safety { .. } => ExitCategory::Input,
        }
    }
}

impl MapError {
    fn analysis(operation: &'static str, error: impl std::fmt::Display) -> Self {
        Self::Analysis { operation, reason: error.to_string() }
    }

    fn safety(operation: &'static str, error: impl std::fmt::Display) -> Self {
        Self::Safety { operation, reason: error.to_string() }
    }
}

enum WalkIssue {
    Traversal(String),
    Safety(String),
}

#[derive(Clone, Debug)]
pub struct MapSettings {
    pub excludes: Vec<String>,
    pub focuses: Vec<String>,
    pub focus_paths: Vec<String>,
    pub map_tokens: usize,
    pub cache_mode: CacheMode,
    pub cache_files: Vec<String>,
}

impl Default for MapSettings {
    fn default() -> Self {
        Self {
            excludes: Vec::new(),
            focuses: Vec::new(),
            focus_paths: Vec::new(),
            map_tokens: DEFAULT_MAP_TOKENS,
            cache_mode: CacheMode::Auto,
            cache_files: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
struct Candidate {
    state: WorktreeState,
    symlink: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ParsedSource {
    symbols: Vec<SourceSymbol>,
    findings: Vec<MapFinding>,
    status: FileAnalysisStatus,
    limitations: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct CacheRecord {
    schema_version: u16,
    tool_version: String,
    repository_root: String,
    scope_path: String,
    path: String,
    language: SourceLanguage,
    query_pack: String,
    fingerprint: String,
    parsed: ParsedSource,
}

#[derive(Debug)]
struct CacheStore {
    root: Option<PathBuf>,
    repository_root: String,
    scope_path: String,
}

#[derive(Debug, Default)]
struct CacheStats {
    hits: usize,
    misses: usize,
    refreshed: Vec<String>,
    stale: Vec<String>,
}

impl CacheStore {
    fn new(repository_root: &Path, scope_path: &str) -> Result<Self> {
        let root = security::cache_root(repository_root)
            .map_err(|error| MapError::safety("resolving the cache root", error))?;
        Ok(Self {
            root,
            repository_root: repository_root.to_string_lossy().into_owned(),
            scope_path: scope_path.to_owned(),
        })
    }

    fn record_path(&self, path: &str, support: &LanguageSupport, fingerprint: Option<&str>) -> Option<PathBuf> {
        let root = self.root.as_ref()?;
        let identity = format!(
            "{CACHE_TOOL_VERSION}\n{}\n{}\n{}\n{}\n{}",
            self.repository_root,
            self.scope_path,
            support.language.label(),
            support.query_pack,
            path
        );
        let records = root.join("maps").join(stable_hash(&identity)).join(stable_hash(path));
        Some(records.join(format!("{}.json", fingerprint.unwrap_or("latest"))))
    }

    fn load(
        &self, path: &str, support: &LanguageSupport, fingerprint: &str, mode: CacheMode,
    ) -> Option<(ParsedSource, bool)> {
        let current = self.record_path(path, support, Some(fingerprint))?;
        if let Some(record) = self.read_record(current.clone(), false, path, support, fingerprint)
            && !record.1
        {
            return Some(record);
        }

        if mode != CacheMode::Manual {
            return None;
        }

        let directory = current.parent()?;
        let root = self.root.as_ref()?;
        if security::cache_path_is_safe(root, directory).is_err() {
            return None;
        }
        let entries = fs::read_dir(directory).ok()?;
        let mut candidates = entries
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|candidate| candidate.extension().is_some_and(|extension| extension == "json"))
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| {
            cache_record_mtime(right)
                .cmp(&cache_record_mtime(left))
                .then_with(|| left.cmp(right))
        });
        candidates
            .into_iter()
            .filter(|candidate| security::cache_path_is_safe(root, candidate).is_ok())
            .find_map(|candidate| self.read_record(candidate, true, path, support, fingerprint))
    }

    fn read_record(
        &self, path: PathBuf, stale: bool, expected_path: &str, support: &LanguageSupport, fingerprint: &str,
    ) -> Option<(ParsedSource, bool)> {
        let bytes = self
            .root
            .as_ref()
            .and_then(|root| security::read_cache_file(root, &path).ok())?;
        let record: CacheRecord = serde_json::from_slice(&bytes).ok()?;
        if record.schema_version != CACHE_SCHEMA_VERSION
            || record.tool_version != CACHE_TOOL_VERSION
            || record.repository_root != self.repository_root
            || record.scope_path != self.scope_path
            || record.path != expected_path
            || record.language != support.language
            || record.query_pack != support.query_pack
        {
            return None;
        }
        Some((record.parsed, stale || record.fingerprint != fingerprint))
    }

    fn write(&self, path: &str, support: &LanguageSupport, fingerprint: &str, parsed: &ParsedSource) -> Option<String> {
        let record_path = self.record_path(path, support, Some(fingerprint))?;
        let record = CacheRecord {
            schema_version: CACHE_SCHEMA_VERSION,
            tool_version: CACHE_TOOL_VERSION.to_owned(),
            repository_root: self.repository_root.clone(),
            scope_path: self.scope_path.clone(),
            path: path.to_owned(),
            language: support.language,
            query_pack: support.query_pack.to_owned(),
            fingerprint: fingerprint.to_owned(),
            parsed: parsed.clone(),
        };
        let bytes = match serde_json::to_vec(&record) {
            Ok(bytes) => bytes,
            Err(error) => return Some(format!("could not serialize the source-map cache record: {error}")),
        };
        let root = self.root.as_ref()?;
        security::write_cache_file(root, &record_path, &bytes)
            .err()
            .map(|error| format!("could not write the source-map cache record: {error}"))
    }
}

#[derive(Clone)]
struct SnippetCandidate {
    path: String,
    language: SourceLanguage,
    symbol: SourceSymbol,
    score: u64,
}

pub fn analyze(path: &Path, settings: &MapSettings) -> Result<MapReport> {
    let selected_path = absolute_path(path)?;
    let repository = security::discover_repository(&selected_path)
        .map_err(|source| MapError::Discovery { path: selected_path.clone(), source })?;
    let scope = security::resolve_scope(&repository, &selected_path).map_err(|error| match error {
        security::ScopeError::Input(reason) => MapError::Input { path: selected_path.clone(), reason },
        security::ScopeError::Safety(error) => MapError::safety("resolving the analysis scope", error),
    })?;
    let repository_root = &scope.repository_root;

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
            scope_path: scope.relative_path.clone(),
        }
    } else {
        CacheStore::new(repository_root, &scope.relative_path)?
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
    collect_tree_files(
        &repository,
        &repository
            .head_tree_id_or_empty()
            .map_err(|error| MapError::analysis("resolving the repository HEAD tree", error))?,
        b"",
        &mut tracked_paths,
    )?;
    collect_index_files(&repository, &mut tracked_paths)?;
    let modified_paths = collect_modified_paths(&repository, repository_root)?;

    let mut candidates = BTreeMap::new();
    for path in tracked_paths
        .into_iter()
        .filter(|path| in_scope(path, &scope.relative_path))
    {
        let state = if modified_paths.contains(&path) { WorktreeState::Modified } else { WorktreeState::Tracked };
        candidates.insert(path, Candidate { state, symlink: false });
    }

    let (visible_paths, visible_errors) = walk_files(&scope.selected_path, repository_root, true);
    for (path, symlink) in &visible_paths {
        if is_git_internal(path) || !in_scope(path, &scope.relative_path) {
            continue;
        }
        candidates
            .entry(path.clone())
            .and_modify(|candidate| candidate.symlink |= *symlink)
            .or_insert(Candidate { state: WorktreeState::Untracked, symlink: *symlink });
    }

    let (all_paths, all_errors) = walk_files(&scope.selected_path, repository_root, false);
    let visible_path_set: BTreeSet<_> = visible_paths.keys().cloned().collect();
    let mut omissions = Vec::new();
    for error in visible_errors.into_iter().chain(all_errors) {
        let (reason, detail) = match error {
            WalkIssue::Traversal(detail) => (OmissionReason::TraversalError, detail),
            WalkIssue::Safety(detail) => (OmissionReason::UnsafePath, detail),
        };
        omissions.push(SourceOmission { path: scope.relative_path.clone(), reason, detail });
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
        let source = match security::read_worktree_file(repository_root, &scope.selected_path, &path) {
            Ok(source) => source,
            Err(security::ReadError::Safety(error)) => {
                let reason = if matches!(error.kind, security::PathSafetyKind::Symlink) {
                    OmissionReason::Symlink
                } else {
                    OmissionReason::UnsafePath
                };
                omissions.push(SourceOmission { path, reason, detail: error.to_string() });
                continue;
            }
            Err(error) => {
                omissions.push(SourceOmission { path, reason: OmissionReason::ReadError, detail: error.to_string() });
                continue;
            }
        };
        let fingerprint = source_fingerprint(&source);
        let forced_refresh = matches!(settings.cache_mode, CacheMode::Always)
            || (settings.cache_mode == CacheMode::Files
                && cache_file_requested(&path, &settings.cache_files, repository_root));
        let (parsed, stale) = if settings.cache_mode != CacheMode::Disabled && !forced_refresh {
            match cache.load(&path, support, &fingerprint, settings.cache_mode) {
                Some((parsed, stale)) => {
                    cache_stats.hits += 1;
                    if stale {
                        cache_stats.stale.push(path.clone());
                    }
                    (parsed, stale)
                }
                None if settings.cache_mode == CacheMode::Manual => {
                    cache_stats.misses += 1;
                    files.push(SourceFile {
                        path: path.clone(),
                        language: support.language,
                        extension: extension_for_path(Path::new(&path)),
                        worktree_state: candidate.state,
                        status: FileAnalysisStatus::Partial,
                        symbols: Vec::new(),
                        limitations: vec![
                            "Manual cache mode found no cached analysis for this file and did not refresh it."
                                .to_owned(),
                        ],
                    });
                    continue;
                }
                None => {
                    cache_stats.misses += 1;
                    let parsed = parse_source(&source, support);
                    if let Some(error) = cache.write(&path, support, &fingerprint, &parsed) {
                        cache_limitations.push(error);
                    }
                    cache_stats.refreshed.push(path.clone());
                    (parsed, false)
                }
            }
        } else {
            let parsed = parse_source(&source, support);
            if settings.cache_mode != CacheMode::Disabled {
                if let Some(error) = cache.write(&path, support, &fingerprint, &parsed) {
                    cache_limitations.push(error);
                }
                cache_stats.refreshed.push(path.clone());
            }
            (parsed, false)
        };
        let ParsedSource { symbols, findings: file_findings, status, mut limitations } = parsed;
        if stale {
            limitations.push(
                "Manual cache mode used a potentially stale source analysis; rerun with `--cache always` to refresh it."
                    .to_owned(),
            );
        }
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
    let mut limitations = if has_non_rust_files {
        vec![
            "Definitions and references are extracted lexically with language-specific Tree-sitter queries; imports, types, macros, and runtime behavior are not resolved."
                .to_owned(),
            "Reference names can have multiple lexical definition candidates; ambiguity is reported rather than treated as a semantic call edge."
                .to_owned(),
            "JavaScript/JSX, TypeScript/TSX, Python, Ruby, Java, and C# use explicit grammar variants; query-pack provenance is reported per language."
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

    if !files.is_empty() {
        limitations.push(
            "Ranking uses deterministic lexical centrality; generic and underscore-prefixed names are downweighted only for ranking and remain available in the full symbol evidence."
                .to_owned(),
        );
    }
    limitations.extend(cache_limitations);

    let edges = build_lexical_edges(&files);
    let ranking = rank_files(&files, &edges, settings);
    let selection = select_snippets(&files, &edges, &ranking, settings.map_tokens, settings);
    let cache_status = cache_status(settings.cache_mode, &cache_stats);
    cache_stats.refreshed.sort();
    cache_stats.stale.sort();

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
        edges,
        ranking,
        selection,
        cache: MapCacheReport {
            mode: settings.cache_mode,
            status: cache_status,
            hits: cache_stats.hits,
            misses: cache_stats.misses,
            refreshed: cache_stats.refreshed,
            stale: cache_stats.stale,
        },
    })
}

fn stable_hash(input: &str) -> String {
    stable_hash_bytes(input.as_bytes())
}

fn stable_hash_bytes(bytes: &[u8]) -> String {
    let mut first = 0xcbf29ce484222325_u64;
    let mut second = 0x9e3779b185ebca87_u64;
    for byte in bytes {
        first ^= u64::from(*byte);
        first = first.wrapping_mul(0x100000001b3);
        second ^= u64::from(*byte).wrapping_add(0x9e37);
        second = second.rotate_left(7).wrapping_mul(0x517cc1b727220a95);
    }
    format!("{first:016x}{second:016x}")
}

fn source_fingerprint(source: &[u8]) -> String {
    stable_hash_bytes(source)
}

fn cache_record_mtime(path: &Path) -> std::time::SystemTime {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .unwrap_or(UNIX_EPOCH)
}

fn cache_file_requested(path: &str, requested: &[String], repository_root: &Path) -> bool {
    requested.iter().any(|requested| {
        normalized_cache_file_path(requested, repository_root).is_some_and(|normalized| normalized == path)
    })
}

fn normalized_cache_file_path(requested: &str, repository_root: &Path) -> Option<String> {
    let requested = requested.trim().replace('\\', "/");
    if requested.is_empty() {
        return None;
    }
    let requested_path = Path::new(&requested);
    let relative = if requested_path.is_absolute() {
        requested_path.strip_prefix(repository_root).ok()?
    } else {
        requested_path
    };
    let mut components = Vec::new();
    for component in relative.components() {
        match component {
            std::path::Component::Normal(component) => components.push(component.to_str()?.to_owned()),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                components.pop()?;
            }
            std::path::Component::RootDir | std::path::Component::Prefix(_) => return None,
        }
    }
    (!components.is_empty()).then(|| components.join("/"))
}

fn cache_status(mode: CacheMode, stats: &CacheStats) -> CacheStatus {
    if mode == CacheMode::Disabled {
        CacheStatus::Disabled
    } else if !stats.stale.is_empty() {
        CacheStatus::Stale
    } else if !stats.refreshed.is_empty() {
        CacheStatus::Refreshed
    } else if stats.hits > 0 {
        CacheStatus::Hit
    } else {
        CacheStatus::Miss
    }
}

fn build_lexical_edges(files: &[SourceFile]) -> Vec<LexicalEdge> {
    let mut definitions = BTreeMap::<String, BTreeSet<String>>::new();
    for file in files {
        for symbol in &file.symbols {
            if symbol.role == SymbolRole::Definition {
                definitions
                    .entry(symbol.name.clone())
                    .or_default()
                    .insert(file.path.clone());
            }
        }
    }

    let mut edges = Vec::new();
    for file in files {
        for symbol in &file.symbols {
            if symbol.role != SymbolRole::Reference {
                continue;
            }
            let Some(candidates) = definitions.get(&symbol.name) else {
                continue;
            };
            let ambiguous = candidates.len() > 1;
            for target in candidates {
                edges.push(LexicalEdge {
                    source: file.path.clone(),
                    target: target.clone(),
                    symbol: symbol.name.clone(),
                    ambiguous,
                    candidates: candidates.iter().cloned().collect(),
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
    });
    edges.dedup();
    edges
}

fn rank_files(files: &[SourceFile], edges: &[LexicalEdge], settings: &MapSettings) -> Vec<FileRank> {
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
            let total_weight = source_edges
                .iter()
                .map(|edge| lexical_weight(&edge.symbol))
                .sum::<f64>();
            if total_weight == 0.0 {
                continue;
            }
            for edge in source_edges {
                if path_set.contains(&edge.target) {
                    let contribution = PAGE_RANK_DAMPING * scores[source] * lexical_weight(&edge.symbol) / total_weight;
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

fn lexical_weight(symbol: &str) -> f64 {
    if is_generic_name(symbol) || symbol.starts_with('_') { 0.25 } else { 1.0 }
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

fn select_snippets(
    files: &[SourceFile], edges: &[LexicalEdge], ranking: &[FileRank], token_budget: usize, settings: &MapSettings,
) -> MapSelection {
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
            let reference_count = edges
                .iter()
                .filter(|edge| edge.target == file.path && edge.symbol == symbol.name)
                .count() as u64;
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
    let full_cost = token_count(&full);
    if full_cost <= budget {
        return Some((candidate.symbol.clone(), full_cost, false));
    }
    let marker = " …";
    if token_count(&format!("{prefix}{marker}")) > budget {
        return None;
    }
    let max_chars = candidate.symbol.context.chars().count();
    let mut best = 0;
    for chars in 0..=max_chars {
        let context = candidate.symbol.context.chars().take(chars).collect::<String>();
        if token_count(&format!("{prefix} {context}{marker}")) <= budget {
            best = chars;
        } else {
            break;
        }
    }
    let context = candidate.symbol.context.chars().take(best).collect::<String>();
    let mut symbol = candidate.symbol.clone();
    symbol.context = format!("{context}{marker}");
    let cost = token_count(&format!("{prefix} {}", symbol.context));
    Some((symbol, cost, true))
}

fn token_count(text: &str) -> usize {
    text.chars().count().div_ceil(4).max(1)
}

fn absolute_path(path: &Path) -> Result<PathBuf> {
    security::absolute_input_path(path).map_err(|error| MapError::analysis("reading the current directory", error))
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
    repository: &gix::Repository, tree_id: &gix::Id<'_>, prefix: &[u8], files: &mut BTreeSet<String>,
) -> Result<()> {
    let tree = repository
        .find_tree(*tree_id)
        .map_err(|error| MapError::analysis("reading the tracked source tree", error))?;
    let mut names = BTreeSet::new();
    for entry in tree.iter() {
        let entry = entry.map_err(|error| MapError::analysis("decoding a tracked tree entry", error))?;
        if !names.insert(entry.filename().as_bytes().to_owned()) {
            return Err(MapError::safety(
                "decoding a tracked tree path",
                security::PathSafetyError { kind: security::PathSafetyKind::Collision },
            ));
        }
        let mut path_bytes = prefix.to_vec();
        if !path_bytes.is_empty() {
            path_bytes.push(b'/');
        }
        path_bytes.extend_from_slice(entry.filename().as_bytes());
        let path = security::validate_repository_path(&path_bytes)
            .map_err(|error| MapError::safety("decoding a tracked tree path", error))?;
        if entry.mode().is_tree() {
            collect_tree_files(repository, &entry.id(), path.as_bytes(), files)?;
        } else {
            if !files.insert(path) {
                return Err(MapError::safety(
                    "decoding a tracked tree path",
                    security::PathSafetyError { kind: security::PathSafetyKind::Collision },
                ));
            }
        }
    }
    Ok(())
}

fn collect_index_files(repository: &gix::Repository, files: &mut BTreeSet<String>) -> Result<()> {
    let index = repository
        .index_or_empty()
        .map_err(|error| MapError::analysis("reading the worktree index", error))?;
    for (path, _) in index.entries_with_paths_by_filter_map(|_, entry| Some(entry.id)) {
        let path = security::validate_repository_path(path.as_bytes())
            .map_err(|error| MapError::safety("decoding a worktree index path", error))?;
        files.insert(path);
    }
    Ok(())
}

fn collect_modified_paths(repository: &gix::Repository, repository_root: &Path) -> Result<BTreeSet<String>> {
    let index = repository
        .index_or_load_from_head_or_empty()
        .map_err(|error| MapError::analysis("loading the worktree index for status", error))?;
    let mut modified = BTreeSet::new();
    // The gix status pipeline is deliberately not used here: it can consult
    // repository attributes and configure clean/smudge/process filters. Byte
    // comparison against the index blob gives the needed modified-path signal
    // without executing repository-controlled programs.
    for (path, id) in index.entries_with_paths_by_filter_map(|_, entry| Some(entry.id)) {
        let path = security::validate_repository_path(path.as_bytes())
            .map_err(|error| MapError::safety("decoding a status path", error))?;
        let worktree = match security::read_worktree_file(repository_root, repository_root, &path) {
            Ok(bytes) => bytes,
            Err(security::ReadError::Safety(_)) => {
                modified.insert(path);
                continue;
            }
            Err(security::ReadError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                modified.insert(path);
                continue;
            }
            Err(security::ReadError::Io(_)) => {
                modified.insert(path);
                continue;
            }
        };
        let blob = repository
            .find_blob(id)
            .map_err(|error| MapError::analysis("reading an index blob for status", error))?;
        if blob.data != worktree {
            modified.insert(path);
        }
    }
    Ok(modified)
}

fn walk_files(root: &Path, repository_root: &Path, standard_filters: bool) -> (BTreeMap<String, bool>, Vec<WalkIssue>) {
    let mut builder = WalkBuilder::new(root);
    builder
        .standard_filters(standard_filters)
        .hidden(false)
        .follow_links(false)
        .sort_by_file_path(|left, right| left.cmp(right));
    let mut files = BTreeMap::new();
    let mut errors = Vec::new();
    let mut native_paths = BTreeMap::<String, PathBuf>::new();
    for item in builder.build() {
        let entry = match item {
            Ok(entry) => entry,
            Err(error) => {
                errors.push(WalkIssue::Traversal(format!(
                    "ignore traversal reported an error: {error}"
                )));
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
        let path = match security::validate_os_relative_path(path) {
            Ok(path) => path,
            Err(error) => {
                errors.push(WalkIssue::Safety(format!("walked path rejected: {error}")));
                continue;
            }
        };
        if native_paths.get(&path).is_some_and(|previous| previous != entry.path()) {
            errors.push(WalkIssue::Safety(
                "two filesystem paths collapsed to one validated repository path".to_owned(),
            ));
            continue;
        }
        native_paths.insert(path.clone(), entry.path().to_owned());
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

fn python_language() -> tree_sitter::Language {
    tree_sitter_python::LANGUAGE.into()
}

fn ruby_language() -> tree_sitter::Language {
    tree_sitter_ruby::LANGUAGE.into()
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

fn java_language() -> tree_sitter::Language {
    tree_sitter_java::LANGUAGE.into()
}

fn c_sharp_language() -> tree_sitter::Language {
    tree_sitter_c_sharp::LANGUAGE.into()
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
    fn python_and_ruby_query_packs_extract_definitions_references_and_nested_scopes() {
        let python = br#"
class Service:
    def run(self, value):
        return helper(value)

def create(value):
    return Service().run(value)
"#;
        let ruby = br#"
module Billing
  class Service
    def run(value)
      helper(value)
    end
  end
end

def build
  Service.new
end
"#;

        let parsed_python = parse_source(python, &PYTHON_SUPPORT);
        let parsed_ruby = parse_source(ruby, &RUBY_SUPPORT);
        assert_eq!(parsed_python.status, FileAnalysisStatus::Complete, "{parsed_python:?}");
        assert_eq!(parsed_ruby.status, FileAnalysisStatus::Complete, "{parsed_ruby:?}");
        assert!(parsed_python.findings.is_empty(), "{parsed_python:?}");
        assert!(parsed_ruby.findings.is_empty(), "{parsed_ruby:?}");

        let python_run = parsed_python
            .symbols
            .iter()
            .find(|symbol| symbol.name == "run" && symbol.role == SymbolRole::Definition)
            .expect("Python method definition");
        assert_eq!(python_run.kind, SymbolKind::Function);
        assert_eq!(python_run.scope, vec!["Service"]);
        assert!(python_run.context.starts_with("def run(self, value):"));
        assert!(
            parsed_python
                .symbols
                .iter()
                .any(|symbol| { symbol.name == "helper" && symbol.role == SymbolRole::Reference })
        );

        let ruby_run = parsed_ruby
            .symbols
            .iter()
            .find(|symbol| symbol.name == "run" && symbol.role == SymbolRole::Definition)
            .expect("Ruby method definition");
        assert_eq!(ruby_run.kind, SymbolKind::Method);
        assert_eq!(ruby_run.scope, vec!["Billing", "Service"]);
        assert!(ruby_run.context.starts_with("def run(value)"));
        assert!(
            parsed_ruby
                .symbols
                .iter()
                .any(|symbol| { symbol.name == "Service" && symbol.role == SymbolRole::Reference })
        );
    }

    #[test]
    fn java_and_c_sharp_query_packs_extract_types_scopes_and_references() {
        let java = br#"
package com.example;
import java.util.List;

public class Service extends BaseService implements Runner {
    private class Hidden {}
    private Helper helper;

    public Result run(Input input) {
        return new Result(helper(input));
    }

    private Result helper(Input input) {
        return input.result();
    }
}

interface Runner {}
"#;
        let c_sharp = br#"
using System;

namespace Example.App {
    public class Service : BaseService, IRunner {
        private class Hidden {}
        private Helper helper;

        public Result Run(Input input) {
            helper.Execute(input);
            return new Result();
        }
    }

    public struct Value {}
    public interface IRunner {}
}
"#;

        let parsed_java = parse_source(java, &JAVA_SUPPORT);
        let parsed_c_sharp = parse_source(c_sharp, &C_SHARP_SUPPORT);
        assert_eq!(parsed_java.status, FileAnalysisStatus::Complete, "{parsed_java:?}");
        assert_eq!(
            parsed_c_sharp.status,
            FileAnalysisStatus::Complete,
            "{parsed_c_sharp:?}"
        );
        assert!(parsed_java.findings.is_empty(), "{parsed_java:?}");
        assert!(parsed_c_sharp.findings.is_empty(), "{parsed_c_sharp:?}");

        assert!(parsed_java.symbols.iter().any(|symbol| {
            symbol.name == "com.example" && symbol.kind == SymbolKind::Module && symbol.role == SymbolRole::Definition
        }));
        assert!(parsed_java.symbols.iter().any(|symbol| {
            symbol.name == "Service" && symbol.kind == SymbolKind::Class && symbol.role == SymbolRole::Definition
        }));
        assert!(parsed_java.symbols.iter().any(|symbol| {
            symbol.name == "Hidden" && symbol.kind == SymbolKind::Class && symbol.role == SymbolRole::Definition
        }));
        assert!(parsed_java.symbols.iter().any(|symbol| {
            symbol.name == "helper" && symbol.kind == SymbolKind::Field && symbol.role == SymbolRole::Definition
        }));
        let java_run = parsed_java
            .symbols
            .iter()
            .find(|symbol| symbol.name == "run" && symbol.role == SymbolRole::Definition)
            .expect("Java method definition");
        assert_eq!(java_run.scope, vec!["Service"]);
        assert!(java_run.context.starts_with("public Result run(Input input)"));
        assert!(
            parsed_java
                .symbols
                .iter()
                .any(|symbol| symbol.name == "Input" && symbol.role == SymbolRole::Reference)
        );
        assert!(parsed_java.symbols.iter().any(|symbol| symbol.name == "helper"
            && symbol.kind == SymbolKind::Method
            && symbol.role == SymbolRole::Reference));

        assert!(parsed_c_sharp.symbols.iter().any(|symbol| {
            symbol.name == "Example.App" && symbol.kind == SymbolKind::Module && symbol.role == SymbolRole::Definition
        }));
        assert!(parsed_c_sharp.symbols.iter().any(|symbol| {
            symbol.name == "Service" && symbol.kind == SymbolKind::Class && symbol.role == SymbolRole::Definition
        }));
        assert!(parsed_c_sharp.symbols.iter().any(|symbol| {
            symbol.name == "Value" && symbol.kind == SymbolKind::Struct && symbol.role == SymbolRole::Definition
        }));
        assert!(parsed_c_sharp.symbols.iter().any(|symbol| {
            symbol.name == "Hidden" && symbol.kind == SymbolKind::Class && symbol.role == SymbolRole::Definition
        }));
        let c_sharp_run = parsed_c_sharp
            .symbols
            .iter()
            .find(|symbol| symbol.name == "Run" && symbol.role == SymbolRole::Definition)
            .expect("C# method definition");
        assert_eq!(c_sharp_run.scope, vec!["Example.App", "Service"]);
        assert!(c_sharp_run.context.starts_with("public Result Run(Input input)"));
        assert!(
            parsed_c_sharp
                .symbols
                .iter()
                .any(|symbol| symbol.name == "Helper" && symbol.role == SymbolRole::Reference)
        );
        assert!(parsed_c_sharp.symbols.iter().any(|symbol| symbol.name == "Execute"
            && symbol.kind == SymbolKind::Method
            && symbol.role == SymbolRole::Reference));
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
        assert_eq!(
            support_for_path(Path::new("module.py")).unwrap().language,
            SourceLanguage::Python
        );
        assert_eq!(
            support_for_path(Path::new("module.pyi")).unwrap().language,
            SourceLanguage::Python
        );
        assert_eq!(
            support_for_path(Path::new("Rakefile.rake")).unwrap().language,
            SourceLanguage::Ruby
        );
        assert_eq!(
            support_for_path(Path::new("Gemfile.gemspec")).unwrap().language,
            SourceLanguage::Ruby
        );
        assert_eq!(
            support_for_path(Path::new("Service.java")).unwrap().language,
            SourceLanguage::Java
        );
        assert_eq!(
            support_for_path(Path::new("Service.cs")).unwrap().language,
            SourceLanguage::CSharp
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
