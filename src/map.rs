use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use gix::bstr::ByteSlice;
use ignore::WalkBuilder;
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tree_sitter::{Node, Parser, Query, QueryCursor, StreamingIterator};

use crate::cli::ExitCategory;
use crate::security;
use crate::{report::*, utils};

const MAX_CONTEXT_CHARS: usize = 180;
const DEFAULT_MAP_TOKENS: usize = 1_000;
const CACHE_SCHEMA_VERSION: u16 = 2;
const CACHE_TOOL_VERSION: &str = "codeplat-map-v8";
const CACHE_MAX_RECORDS_PER_REPOSITORY: usize = 256;
const CACHE_MAX_BYTES_PER_REPOSITORY: u64 = 32 * 1024 * 1024;
const CACHE_MAX_AGE_SECONDS: u64 = 30 * 24 * 60 * 60;
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
    "use_declaration",
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
    "using_directive",
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

/// Return the compiled language/query-pack contract without inspecting a repository.
pub fn language_capabilities() -> Vec<LanguageCapability> {
    LANGUAGE_SUPPORT
        .iter()
        .map(|support| LanguageCapability {
            language: support.language,
            extensions: support
                .extensions
                .iter()
                .map(|extension| (*extension).to_owned())
                .collect(),
            grammar: grammar_name(support.language).to_owned(),
            grammar_version: grammar_version(support.language).to_owned(),
            query_pack: support.query_pack.to_owned(),
            query_pack_version: query_pack_version(support.query_pack).to_owned(),
            definitions: !support.definitions.trim().is_empty(),
            references: !support.references.trim().is_empty(),
        })
        .collect()
}

/// Compile every embedded query against its grammar. This is deliberately
/// independent of repository discovery so `capabilities` and `doctor` stay
/// read-only support diagnostics.
pub fn validate_query_packs() -> std::result::Result<(), String> {
    for support in LANGUAGE_SUPPORT {
        let language = (support.grammar)();
        Query::new(&language, support.definitions)
            .map_err(|error| format!("{} definition query: {error}", support.language.label()))?;
        Query::new(&language, support.references)
            .map_err(|error| format!("{} reference query: {error}", support.language.label()))?;
    }
    Ok(())
}

fn grammar_name(language: SourceLanguage) -> &'static str {
    match language {
        SourceLanguage::Rust => "tree-sitter-rust",
        SourceLanguage::JavaScript | SourceLanguage::JavaScriptJsx => "tree-sitter-javascript",
        SourceLanguage::TypeScript | SourceLanguage::TypeScriptTsx => "tree-sitter-typescript",
        SourceLanguage::Python => "tree-sitter-python",
        SourceLanguage::Ruby => "tree-sitter-ruby",
        SourceLanguage::Java => "tree-sitter-java",
        SourceLanguage::CSharp => "tree-sitter-c-sharp",
    }
}

fn grammar_version(language: SourceLanguage) -> &'static str {
    match language {
        SourceLanguage::Rust => "0.24.2",
        SourceLanguage::JavaScript | SourceLanguage::JavaScriptJsx => "0.25.0",
        SourceLanguage::TypeScript | SourceLanguage::TypeScriptTsx => "0.23.2",
        SourceLanguage::Python => "0.25.0",
        SourceLanguage::Ruby => "0.23.1",
        SourceLanguage::Java => "0.23.5",
        SourceLanguage::CSharp => "0.23.5",
    }
}

fn query_pack_version(query_pack: &str) -> &str {
    query_pack.rsplit_once('-').map_or(query_pack, |(_, version)| version)
}

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
    pub profile: AnalysisProfile,
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
            profile: AnalysisProfile::Compact,
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
    path: String,
    language: SourceLanguage,
    query_pack: String,
    query_digest: String,
    fingerprint: String,
    created_at: u128,
    parsed: ParsedSource,
}

#[derive(Debug)]
struct CacheStore {
    root: Option<PathBuf>,
    repository_root: String,
    repository_id: String,
}

#[derive(Debug, Default)]
struct CacheStats {
    matched: usize,
    unmatched: usize,
    unavailable: usize,
    hits: usize,
    misses: usize,
    refreshed: Vec<String>,
    stale: Vec<String>,
}

#[derive(Debug)]
struct CacheLookup {
    parsed: ParsedSource,
    stale: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CacheCommand {
    Path,
    Status,
    Prune,
    Clear,
}

#[derive(Clone, Debug, Serialize)]
pub struct CacheControlReport {
    pub operation: String,
    pub path: Option<String>,
    pub exists: bool,
    pub repositories: usize,
    pub records: usize,
    pub bytes: u64,
    pub removed_records: usize,
    pub removed_bytes: u64,
    pub max_records_per_repository: usize,
    pub max_bytes_per_repository: u64,
    pub max_age_seconds: u64,
}

#[derive(Debug)]
struct CacheFileInfo {
    path: PathBuf,
    bytes: u64,
    timestamp: u128,
    repository_id: Option<String>,
}

/// Inspect or mutate only Codeplat's configured cache root. This deliberately
/// does not discover a Git repository or access a worktree.
pub fn cache_control(command: CacheCommand) -> Result<CacheControlReport> {
    let root =
        security::configured_cache_root().map_err(|error| MapError::safety("resolving the cache root", error))?;
    let operation = match command {
        CacheCommand::Path => "path",
        CacheCommand::Status => "status",
        CacheCommand::Prune => "prune",
        CacheCommand::Clear => "clear",
    };
    let Some(root) = root else {
        return Ok(CacheControlReport {
            operation: operation.to_owned(),
            path: None,
            exists: false,
            repositories: 0,
            records: 0,
            bytes: 0,
            removed_records: 0,
            removed_bytes: 0,
            max_records_per_repository: CACHE_MAX_RECORDS_PER_REPOSITORY,
            max_bytes_per_repository: CACHE_MAX_BYTES_PER_REPOSITORY,
            max_age_seconds: CACHE_MAX_AGE_SECONDS,
        });
    };

    let exists = root.is_dir();
    let mut removed_records = 0;
    let mut removed_bytes = 0;
    if exists && command == CacheCommand::Prune {
        let repositories = root.join("repositories");
        for repository in direct_directories(&repositories)? {
            let (removed, bytes) = prune_cache_directory(&root, &repository)
                .map_err(|error| MapError::analysis("pruning the cache", error))?;
            removed_records += removed;
            removed_bytes += bytes;
        }
    } else if exists && command == CacheCommand::Clear {
        let repositories = root.join("repositories");
        for repository in direct_directories(&repositories)? {
            if security::cache_path_is_safe(&root, &repository).is_err() {
                continue;
            }
            let info = collect_cache_files(&root, &repository)?;
            removed_records += info.len();
            removed_bytes += info.iter().map(|entry| entry.bytes).sum::<u64>();
            fs::remove_dir_all(repository).map_err(|error| MapError::analysis("clearing the cache", error))?;
        }
    }

    let files = if root.is_dir() { collect_cache_files(&root, &root.join("repositories"))? } else { Vec::new() };
    let repositories = files
        .iter()
        .filter_map(|entry| entry.repository_id.as_deref())
        .collect::<BTreeSet<_>>()
        .len();
    Ok(CacheControlReport {
        operation: operation.to_owned(),
        path: Some(root.to_string_lossy().into_owned()),
        exists,
        repositories,
        records: files.len(),
        bytes: files.iter().map(|entry| entry.bytes).sum(),
        removed_records,
        removed_bytes,
        max_records_per_repository: CACHE_MAX_RECORDS_PER_REPOSITORY,
        max_bytes_per_repository: CACHE_MAX_BYTES_PER_REPOSITORY,
        max_age_seconds: CACHE_MAX_AGE_SECONDS,
    })
}

fn direct_directories(path: &Path) -> Result<Vec<PathBuf>> {
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(MapError::analysis("reading the cache", error)),
    };
    let mut directories = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            entry
                .file_type()
                .ok()
                .filter(|file_type| file_type.is_dir() && !file_type.is_symlink())
                .map(|_| entry.path())
        })
        .collect::<Vec<_>>();
    directories.sort();
    Ok(directories)
}

fn collect_cache_files(root: &Path, directory: &Path) -> Result<Vec<CacheFileInfo>> {
    if !directory.is_dir() {
        return Ok(Vec::new());
    }
    if security::cache_path_is_safe(root, directory).is_err() {
        return Ok(Vec::new());
    }
    let repository_id = directory
        .strip_prefix(root.join("repositories"))
        .ok()
        .and_then(|relative| relative.components().next())
        .and_then(|component| component.as_os_str().to_str())
        .map(str::to_owned);
    let mut files = Vec::new();
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(MapError::analysis("reading the cache", error)),
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(MapError::analysis("reading the cache", error)),
        };
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(MapError::analysis("reading the cache", error)),
        };
        let path = entry.path();
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            files.extend(collect_cache_files(root, &path)?);
        } else if file_type.is_file() && path.extension().is_some_and(|extension| extension == "json") {
            if security::cache_path_is_safe(root, &path).is_err() {
                continue;
            }
            let metadata = match entry.metadata() {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(MapError::analysis("reading cache metadata", error)),
            };
            let timestamp = security::read_cache_file(root, &path)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<CacheRecord>(&bytes).ok())
                .map(|record| record.created_at)
                .unwrap_or_else(|| {
                    metadata
                        .modified()
                        .ok()
                        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
                        .map(|duration| duration.as_nanos())
                        .unwrap_or_default()
                });
            files.push(CacheFileInfo { path, bytes: metadata.len(), timestamp, repository_id: repository_id.clone() });
        }
    }
    Ok(files)
}

fn prune_cache_directory(root: &Path, repository: &Path) -> std::result::Result<(usize, u64), String> {
    let mut files = collect_cache_files(root, repository).map_err(|error| error.to_string())?;
    files.sort_by(|left, right| {
        right
            .timestamp
            .cmp(&left.timestamp)
            .then_with(|| left.path.cmp(&right.path))
    });
    let cutoff = unix_timestamp().saturating_sub(u128::from(CACHE_MAX_AGE_SECONDS) * 1_000_000_000);
    let mut remove = files
        .iter()
        .filter(|entry| entry.timestamp < cutoff)
        .map(|entry| entry.path.clone())
        .collect::<BTreeSet<_>>();
    let mut remaining = files
        .iter()
        .filter(|entry| !remove.contains(&entry.path))
        .collect::<Vec<_>>();
    let mut remaining_bytes = remaining.iter().map(|entry| entry.bytes).sum::<u64>();
    while remaining.len() > CACHE_MAX_RECORDS_PER_REPOSITORY || remaining_bytes > CACHE_MAX_BYTES_PER_REPOSITORY {
        let Some(entry) = remaining.pop() else {
            break;
        };
        remaining_bytes = remaining_bytes.saturating_sub(entry.bytes);
        remove.insert(entry.path.clone());
    }
    let mut removed_bytes = 0;
    for path in &remove {
        if security::cache_path_is_safe(root, path).is_err() {
            continue;
        }
        if let Ok(metadata) = fs::symlink_metadata(path) {
            removed_bytes += metadata.len();
        }
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok((remove.len(), removed_bytes))
}

impl CacheStore {
    fn new(repository_root: &Path) -> Result<Self> {
        let root = security::cache_root(repository_root)
            .map_err(|error| MapError::safety("resolving the cache root", error))?;
        let repository_root = repository_root.to_string_lossy().into_owned();
        Ok(Self { root, repository_id: digest_hex(repository_root.as_bytes()), repository_root })
    }

    fn repository_path(&self) -> Option<PathBuf> {
        self.root
            .as_ref()
            .map(|root| root.join("repositories").join(&self.repository_id))
    }

    fn record_directory(&self, path: &str, support: &LanguageSupport) -> Option<PathBuf> {
        let root = self.root.as_ref()?;
        let identity = format!(
            "{CACHE_TOOL_VERSION}\n{}\n{}\n{}\n{}",
            self.repository_root,
            path,
            support.language.label(),
            query_digest(support),
        );
        Some(
            root.join("repositories")
                .join(&self.repository_id)
                .join("files")
                .join(digest_hex(identity.as_bytes())),
        )
    }

    fn record_path(&self, path: &str, support: &LanguageSupport, fingerprint: &str) -> Option<PathBuf> {
        Some(
            self.record_directory(path, support)?
                .join(format!("{fingerprint}.json")),
        )
    }

    fn load(&self, path: &str, support: &LanguageSupport, fingerprint: &str, mode: CacheMode) -> Option<CacheLookup> {
        let directory = self.record_directory(path, support)?;
        let root = self.root.as_ref()?;
        if security::cache_path_is_safe(root, &directory).is_err() {
            return None;
        }

        if mode != CacheMode::Manual {
            let current = self.record_path(path, support, fingerprint)?;
            let record = self.read_record(current, path, support)?;
            return (record.fingerprint == fingerprint).then_some(CacheLookup { parsed: record.parsed, stale: false });
        }

        let entries = fs::read_dir(directory).ok()?;
        let mut candidates = entries
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|candidate| candidate.extension().is_some_and(|extension| extension == "json"))
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| {
            self.cache_record_created_at(right)
                .cmp(&self.cache_record_created_at(left))
                .then_with(|| cache_record_mtime(right).cmp(&cache_record_mtime(left)))
                .then_with(|| left.cmp(right))
        });
        candidates
            .into_iter()
            .filter(|candidate| security::cache_path_is_safe(root, candidate).is_ok())
            .find_map(|candidate| {
                let record = self.read_record(candidate, path, support)?;
                Some(CacheLookup { parsed: record.parsed, stale: record.fingerprint != fingerprint })
            })
    }

    fn read_record(&self, path: PathBuf, expected_path: &str, support: &LanguageSupport) -> Option<CacheRecord> {
        let bytes = self
            .root
            .as_ref()
            .and_then(|root| security::read_cache_file(root, &path).ok())?;
        let record: CacheRecord = serde_json::from_slice(&bytes).ok()?;
        if record.schema_version != CACHE_SCHEMA_VERSION
            || record.tool_version != CACHE_TOOL_VERSION
            || record.repository_root != self.repository_root
            || record.path != expected_path
            || record.language != support.language
            || record.query_pack != support.query_pack
            || record.query_digest != query_digest(support)
        {
            return None;
        }
        Some(record)
    }

    fn cache_record_created_at(&self, path: &Path) -> u128 {
        self.root
            .as_ref()
            .and_then(|root| security::read_cache_file(root, path).ok())
            .and_then(|bytes| serde_json::from_slice::<CacheRecord>(&bytes).ok())
            .map(|record| record.created_at)
            .unwrap_or_default()
    }

    fn write(&self, path: &str, support: &LanguageSupport, fingerprint: &str, parsed: &ParsedSource) -> Option<String> {
        let record_path = self.record_path(path, support, fingerprint)?;
        let record = CacheRecord {
            schema_version: CACHE_SCHEMA_VERSION,
            tool_version: CACHE_TOOL_VERSION.to_owned(),
            repository_root: self.repository_root.clone(),
            path: path.to_owned(),
            language: support.language,
            query_pack: support.query_pack.to_owned(),
            query_digest: query_digest(support),
            fingerprint: fingerprint.to_owned(),
            created_at: unix_timestamp(),
            parsed: parsed.clone(),
        };
        let bytes = match serde_json::to_vec(&record) {
            Ok(bytes) => bytes,
            Err(error) => return Some(format!("could not serialize the source-map cache record: {error}")),
        };
        let root = self.root.as_ref()?;
        if let Err(error) = security::write_cache_file(root, &record_path, &bytes) {
            return Some(format!("could not write the source-map cache record: {error}"));
        }
        self.prune_repository()
            .err()
            .map(|error| format!("could not prune source-map cache records: {error}"))
    }

    fn prune_repository(&self) -> std::result::Result<(usize, u64), String> {
        let Some(root) = self.root.as_ref() else {
            return Ok((0, 0));
        };
        let Some(repository_path) = self.repository_path() else {
            return Ok((0, 0));
        };
        prune_cache_directory(root, &repository_path)
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
    collect_tree_files(
        &repository,
        &repository
            .head_tree_id_or_empty()
            .map_err(|error| MapError::analysis("resolving the repository HEAD tree", error))?,
        b"",
        &mut tracked_paths,
        limits.max_files,
        limits.max_syntax_depth,
    )?;
    collect_index_files(&repository, &mut tracked_paths, limits.max_files)?;
    let modified_paths = collect_modified_paths(&repository, repository_root, limits.max_file_bytes)?;

    let mut candidates = BTreeMap::new();
    for path in tracked_paths
        .into_iter()
        .filter(|path| in_scope(path, &scope.relative_path))
    {
        let state = if modified_paths.contains(&path) { WorktreeState::Modified } else { WorktreeState::Tracked };
        candidates.insert(path, Candidate { state, symlink: false });
    }

    let (visible_paths, visible_errors) = walk_files(&scope.selected_path, repository_root, true, limits.max_files);
    for (path, symlink) in &visible_paths {
        if is_git_internal(path) || !in_scope(path, &scope.relative_path) {
            continue;
        }
        candidates
            .entry(path.clone())
            .and_modify(|candidate| candidate.symlink |= *symlink)
            .or_insert(Candidate { state: WorktreeState::Untracked, symlink: *symlink });
    }

    let (all_paths, all_errors) = walk_files(&scope.selected_path, repository_root, false, limits.max_files);
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

    let inventory = inventory(&candidates);
    if candidates.len() > limits.max_files {
        let kept = candidates
            .keys()
            .take(limits.max_files)
            .cloned()
            .collect::<BTreeSet<_>>();
        for path in candidates.keys().filter(|path| !kept.contains(*path)) {
            omissions.push(SourceOmission {
                path: path.clone(),
                reason: OmissionReason::TraversalError,
                detail: format!(
                    "The file-count resource limit ({}) was reached before this path could be analyzed.",
                    limits.max_files
                ),
            });
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
            omissions.push(SourceOmission {
                path: scope.relative_path.clone(),
                reason: OmissionReason::TraversalError,
                detail: format!(
                    "The analysis time resource limit ({} ms) was reached before this path could be analyzed.",
                    limits.max_elapsed_ms
                ),
            });
            work_limit_reached = true;
            break;
        }
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
                omissions.push(SourceOmission { path, reason, detail: error.to_string() });
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
                omissions.push(SourceOmission { path, reason, detail: error.to_string() });
                continue;
            }
        };
        if total_source_bytes.saturating_add(source.len()) > limits.max_total_bytes {
            omissions.push(SourceOmission {
                path,
                reason: OmissionReason::Oversized,
                detail: format!(
                    "The total source-byte resource limit ({}) was reached; this file was not analyzed.",
                    limits.max_total_bytes
                ),
            });
            continue;
        }
        total_source_bytes = total_source_bytes.saturating_add(source.len());
        if source.contains(&0) || std::str::from_utf8(&source).is_err() {
            omissions.push(SourceOmission {
                path,
                reason: OmissionReason::Binary,
                detail: "Binary or non-UTF-8 source input is not parsed by the Tree-sitter map.".to_owned(),
            });
            continue;
        }
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
                        omissions.push(SourceOmission {
                            path: path.clone(),
                            reason: OmissionReason::CacheUnavailable,
                            detail: if settings.cache_mode == CacheMode::Manual {
                                "Manual cache mode found no usable record for this file and did not refresh it."
                            } else {
                                "Files cache mode did not refresh this unrequested file because no current cache record was available."
                            }
                            .to_owned(),
                        });
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
            "JavaScript/JSX, TypeScript/TSX, Python, Ruby, Java, and C# use explicit grammar variants; query-pack provenance is reported per language."
                .to_owned(),
            "Tracked files are eligible even when ignore rules match them; ignored untracked files are omitted and recorded."
                .to_owned(),
        ]
    } else {
        vec![
            "Rust definitions and references are extracted lexically; only explicit same-file call evidence is graphed, and imports, types, macros, and runtime behavior are not semantically resolved."
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
            matched: cache_stats.matched,
            unmatched: cache_stats.unmatched,
            unavailable: cache_stats.unavailable,
            refreshed: cache_stats.refreshed,
            stale: cache_stats.stale,
        },
        collections: MapCollections {
            files: CollectionSummary::complete(0),
            symbols: CollectionSummary::complete(0),
            omissions: CollectionSummary::complete(0),
            findings: CollectionSummary::complete(0),
            edges: CollectionSummary::complete(0),
            ranking: CollectionSummary::complete(0),
            snippets: CollectionSummary::complete(0),
        },
    };
    bound_map_report(&mut report, settings.profile, &limits);
    Ok(report)
}

fn bound_map_report(report: &mut MapReport, profile: AnalysisProfile, limits: &ReportLimits) {
    let files_total = report.files.len();
    let symbols_total = report.files.iter().map(|file| file.symbols.len()).sum::<usize>();
    let omissions_total = report.omissions.len();
    let findings_total = report.findings.len();
    let edges_total = report.edges.len();
    let ranking_total = report.ranking.len();
    let snippets_total = report.selection.snippets.len();
    let (file_limit, symbols_per_file, omission_limit, finding_limit, edge_limit, ranking_limit) = match profile {
        AnalysisProfile::Compact => (32, 16, 8, 32, 32, 16),
        AnalysisProfile::Evidence => (
            limits.max_files,
            limits.max_symbols_per_file,
            limits.max_findings,
            limits.max_findings,
            limits.max_edges,
            limits.max_files,
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
    };
    if [
        report.collections.files.truncated,
        report.collections.symbols.truncated,
        report.collections.omissions.truncated,
        report.collections.findings.truncated,
        report.collections.edges.truncated,
        report.collections.ranking.truncated,
        report.collections.snippets.truncated,
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

fn collection_summary(
    total: usize, returned: usize, profile: AnalysisProfile, reason: TruncationReason,
) -> CollectionSummary {
    if returned >= total {
        CollectionSummary::complete(total)
    } else {
        let reason = if profile == AnalysisProfile::Compact && reason == TruncationReason::CollectionLimit {
            TruncationReason::CollectionLimit
        } else {
            reason
        };
        CollectionSummary { total, returned, truncated: true, reason: Some(reason) }
    }
}

fn bounded_text(text: &str, max_chars: usize) -> String {
    let mut output = text.chars().take(max_chars).collect::<String>();
    if text.chars().count() > max_chars {
        output.push('…');
    }
    output
}

fn digest_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(output, "{byte:02x}").expect("writing to a String cannot fail");
    }
    output
}

fn source_fingerprint(source: &[u8]) -> String {
    digest_hex(source)
}

fn query_digest(support: &LanguageSupport) -> String {
    let mut identity = Vec::new();
    identity.extend_from_slice(support.query_pack.as_bytes());
    identity.push(0);
    identity.extend_from_slice(support.definitions.as_bytes());
    identity.push(0);
    identity.extend_from_slice(support.references.as_bytes());
    digest_hex(&identity)
}

fn unix_timestamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}

fn cache_record_mtime(path: &Path) -> std::time::SystemTime {
    fs::symlink_metadata(path)
        .and_then(|metadata| metadata.modified())
        .unwrap_or(UNIX_EPOCH)
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

fn build_lexical_edges(files: &[SourceFile], max_candidates: usize, max_edges: usize) -> Vec<LexicalEdge> {
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
                    .map(|symbol| (symbol.name.clone(), import_module_hints(&symbol.context)))
                    .collect::<Vec<_>>(),
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
            let imported = imports
                .get(&file.path)
                .into_iter()
                .flatten()
                .find(|(name, _)| name == &symbol.name);
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
            } else {
                let Some((_, hints)) = imported else {
                    // A cross-file bare name is not evidence of a dependency.
                    continue;
                };
                let module_candidates = all_candidates
                    .iter()
                    .filter(|(path, _)| module_path_matches(path, hints))
                    .cloned()
                    .collect::<Vec<_>>();
                if module_candidates.is_empty() {
                    (
                        all_candidates.clone(),
                        LexicalResolutionReason::ImportedName,
                        ConfidenceTier::Medium,
                    )
                } else {
                    (
                        module_candidates,
                        LexicalResolutionReason::ImportedModule,
                        ConfidenceTier::High,
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

fn import_module_hints(context: &str) -> Vec<String> {
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
        hints.push(normalize_module_hint(&value));
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
        direct_match || normalized == module || normalized.ends_with(&format!("/{module}"))
    })
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

fn select_snippets(
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

fn absolute_path(path: &Path) -> Result<PathBuf> {
    security::absolute_input_path(path).map_err(|error| MapError::analysis("reading the current directory", error))
}

fn repository_head_snapshot(repository: &gix::Repository) -> Result<HeadSnapshot> {
    let reference = repository
        .head_name()
        .map_err(|error| MapError::analysis("resolving HEAD reference", error))?
        .map(|name| name.as_bstr().to_str_lossy().into_owned());
    let oid = repository.head_id().ok().map(|id| id.to_string());
    Ok(HeadSnapshot {
        detached: reference.is_none() && oid.is_some(),
        unborn: reference.is_some() && oid.is_none(),
        reference,
        oid,
    })
}

fn worktree_snapshot(inventory: (usize, usize, usize)) -> WorktreeSnapshot {
    let (tracked_files, modified_files, untracked_files) = inventory;
    let state = match (modified_files > 0, untracked_files > 0) {
        (false, false) => WorktreeSnapshotState::Clean,
        (true, false) => WorktreeSnapshotState::Modified,
        (false, true) => WorktreeSnapshotState::Untracked,
        (true, true) => WorktreeSnapshotState::Mixed,
    };
    WorktreeSnapshot { state, observed: true, tracked_files, modified_files, untracked_files, detail: None }
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
    repository: &gix::Repository, tree_id: &gix::Id<'_>, prefix: &[u8], files: &mut BTreeSet<String>, max_files: usize,
    max_depth: usize,
) -> Result<()> {
    let mut stack = vec![(tree_id.detach(), prefix.to_vec(), 0usize)];
    while let Some((tree_id, prefix, depth)) = stack.pop() {
        if files.len() >= max_files || depth > max_depth {
            break;
        }
        let tree = repository
            .find_tree(tree_id)
            .map_err(|error| MapError::analysis("reading the tracked source tree", error))?;
        let mut names = BTreeSet::new();
        let mut entries = Vec::new();
        for entry in tree.iter() {
            if entries.len() >= max_files {
                break;
            }
            let entry = entry.map_err(|error| MapError::analysis("decoding a tracked tree entry", error))?;
            let name = entry.filename().as_bytes().to_owned();
            if !names.insert(name) {
                return Err(MapError::safety(
                    "decoding a tracked tree path",
                    security::PathSafetyError { kind: security::PathSafetyKind::Collision },
                ));
            }
            entries.push(entry);
        }
        entries.reverse();
        for entry in entries {
            if files.len() >= max_files {
                break;
            }
            let mut path_bytes = prefix.clone();
            if !path_bytes.is_empty() {
                path_bytes.push(b'/');
            }
            path_bytes.extend_from_slice(entry.filename().as_bytes());
            let path = security::validate_repository_path(&path_bytes)
                .map_err(|error| MapError::safety("decoding a tracked tree path", error))?;
            if entry.mode().is_tree() {
                stack.push((entry.id().detach(), path.into_bytes(), depth.saturating_add(1)));
            } else if !files.insert(path) {
                return Err(MapError::safety(
                    "decoding a tracked tree path",
                    security::PathSafetyError { kind: security::PathSafetyKind::Collision },
                ));
            }
        }
    }
    Ok(())
}

fn collect_index_files(repository: &gix::Repository, files: &mut BTreeSet<String>, max_files: usize) -> Result<()> {
    let index = repository
        .index_or_empty()
        .map_err(|error| MapError::analysis("reading the worktree index", error))?;
    for (path, _) in index.entries_with_paths_by_filter_map(|_, entry| Some(entry.id)) {
        if files.len() >= max_files {
            break;
        }
        let path = security::validate_repository_path(path.as_bytes())
            .map_err(|error| MapError::safety("decoding a worktree index path", error))?;
        files.insert(path);
    }
    Ok(())
}

fn collect_modified_paths(
    repository: &gix::Repository, repository_root: &Path, max_file_bytes: usize,
) -> Result<BTreeSet<String>> {
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
        let worktree =
            match security::read_worktree_file_limited(repository_root, repository_root, &path, max_file_bytes) {
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

fn walk_files(
    root: &Path, repository_root: &Path, standard_filters: bool, max_entries: usize,
) -> (BTreeMap<String, bool>, Vec<WalkIssue>) {
    let mut builder = WalkBuilder::new(root);
    builder
        .standard_filters(standard_filters)
        .hidden(false)
        .follow_links(false)
        .sort_by_file_path(|left, right| left.cmp(right));
    let filter_repository_root = repository_root.to_owned();
    builder.filter_entry(move |entry| {
        if entry.depth() == 0 || !entry.file_type().is_some_and(|file_type| file_type.is_dir()) {
            return true;
        }
        !pruned_directory(entry.path(), &filter_repository_root)
    });
    let mut files = BTreeMap::new();
    let mut errors = Vec::new();
    let mut native_paths = BTreeMap::<String, PathBuf>::new();
    for item in builder.build() {
        if files.len() >= max_entries {
            errors.push(WalkIssue::Traversal(format!(
                "file inventory reached the {}-path limit; deeper paths were not visited",
                max_entries
            )));
            break;
        }
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

fn pruned_directory(path: &Path, repository_root: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return true;
    };
    if matches!(
        name,
        ".git" | "target" | "node_modules" | "vendor" | "dist" | "build" | "out" | "coverage"
    ) {
        return true;
    }
    path != repository_root
        && fs::symlink_metadata(path.join(".git")).is_ok_and(|metadata| metadata.is_dir() || metadata.is_file())
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

#[cfg(test)]
fn parse_source(source: &[u8], support: &LanguageSupport) -> ParsedSource {
    let mut analyzer = LanguageAnalyzer::new(support);
    parse_source_with_analyzer(
        source,
        &mut analyzer,
        &ReportLimits::for_profile(AnalysisProfile::Evidence),
    )
}

struct LanguageAnalyzer<'a> {
    support: &'a LanguageSupport,
    parser: Parser,
    parser_error: Option<String>,
    definition_query: Option<Query>,
    definition_error: Option<String>,
    reference_query: Option<Query>,
    reference_error: Option<String>,
}

impl<'a> LanguageAnalyzer<'a> {
    fn new(support: &'a LanguageSupport) -> Self {
        let language = (support.grammar)();
        let mut parser = Parser::new();
        let parser_error = parser.set_language(&language).err().map(|error| error.to_string());
        let (definition_query, definition_error) = match Query::new(&language, support.definitions) {
            Ok(query) => (Some(query), None),
            Err(error) => (None, Some(error.to_string())),
        };
        let (reference_query, reference_error) = match Query::new(&language, support.references) {
            Ok(query) => (Some(query), None),
            Err(error) => (None, Some(error.to_string())),
        };
        Self { support, parser, parser_error, definition_query, definition_error, reference_query, reference_error }
    }
}

fn parse_source_with_analyzer(
    source: &[u8], analyzer: &mut LanguageAnalyzer<'_>, limits: &ReportLimits,
) -> ParsedSource {
    let support = analyzer.support;
    let mut findings = Vec::new();
    if let Some(error) = &analyzer.parser_error {
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
    let Some(tree) = analyzer.parser.parse(source, None) else {
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
    let mut symbols_truncated = false;
    if let Some(error) = &analyzer.definition_error {
        findings.push(MapFinding {
            kind: MapFindingKind::QueryError,
            path: String::new(),
            location: None,
            detail: format!(
                "Could not compile the {} definition query in query pack `{}`: {error}.",
                support.language.display_label(),
                support.query_pack
            ),
        });
        query_failed = true;
    } else if let Some(definition_query) = analyzer.definition_query.as_ref() {
        let mut matches = cursor.matches(definition_query, tree.root_node(), source);
        'definitions: while let Some(query_match) = matches.next() {
            for capture in query_match.captures {
                if symbols.len() >= limits.max_symbols_per_file {
                    symbols_truncated = true;
                    break 'definitions;
                }
                let node = capture.node;
                definition_nodes.insert(node.id());
                symbols.push(symbol_from_capture(
                    node,
                    capture_name(definition_query, capture.index),
                    SymbolRole::Definition,
                    source,
                    support,
                ));
            }
        }
    } else {
        query_failed = true;
    }
    if let Some(error) = &analyzer.reference_error {
        findings.push(MapFinding {
            kind: MapFindingKind::QueryError,
            path: String::new(),
            location: None,
            detail: format!(
                "Could not compile the {} reference query in query pack `{}`: {error}.",
                support.language.display_label(),
                support.query_pack
            ),
        });
        query_failed = true;
    } else if let Some(reference_query) = analyzer.reference_query.as_ref() {
        let mut matches = cursor.matches(reference_query, tree.root_node(), source);
        'references: while let Some(query_match) = matches.next() {
            for capture in query_match.captures {
                if symbols.len() >= limits.max_symbols_per_file {
                    symbols_truncated = true;
                    break 'references;
                }
                let node = capture.node;
                if definition_nodes.contains(&node.id()) {
                    continue;
                }
                symbols.push(symbol_from_capture(
                    node,
                    capture_name(reference_query, capture.index),
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

    let syntax_truncated = collect_parse_findings(
        tree.root_node(),
        source,
        &mut findings,
        limits.max_syntax_depth,
        limits.max_findings,
    );
    let status = if tree.root_node().has_error() || query_failed || symbols_truncated || syntax_truncated {
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
    if symbols_truncated {
        limitations.push(format!(
            "The per-file symbol limit ({}) was reached; additional syntax captures were not visited.",
            limits.max_symbols_per_file
        ));
    }
    if syntax_truncated {
        limitations.push(format!(
            "Syntax traversal reached the depth limit ({}); deeper nodes were omitted.",
            limits.max_syntax_depth
        ));
    }
    if findings.len() > limits.max_findings {
        findings.truncate(limits.max_findings);
    }
    ParsedSource { symbols, findings, status, limitations }
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
    let kind = symbol_kind(capture_name);
    SourceSymbol {
        name: text_for_node(node, source),
        kind,
        role,
        scope: scope_for_node(scope_start, source, support.scope_kinds),
        location: SourceLocation::from(node),
        context: context_snippet(node, source, support.declaration_kinds),
        visibility: visibility_for_node(declaration, role, source),
        evidence: evidence_for_node(node, capture_name, role, kind),
    }
}

fn evidence_for_node(node: Node<'_>, capture_name: &str, role: SymbolRole, kind: SymbolKind) -> SymbolEvidence {
    if role == SymbolRole::Definition {
        return if kind == SymbolKind::Import { SymbolEvidence::Import } else { SymbolEvidence::Declaration };
    }
    if capture_name.ends_with(".type") || kind == SymbolKind::Type {
        SymbolEvidence::TypeReference
    } else if capture_name.ends_with(".field") || kind == SymbolKind::Field {
        SymbolEvidence::MemberReference
    } else if capture_name.ends_with(".method") || kind == SymbolKind::Method || is_call_like(node) {
        SymbolEvidence::Call
    } else {
        SymbolEvidence::BareReference
    }
}

fn is_call_like(node: Node<'_>) -> bool {
    let mut current = Some(node);
    while let Some(candidate) = current {
        if matches!(
            candidate.kind(),
            "call"
                | "call_expression"
                | "function_call"
                | "invocation_expression"
                | "method_invocation"
                | "new_expression"
                | "object_creation_expression"
                | "class_instance_creation_expression"
        ) {
            return true;
        }
        if candidate.kind() == "source_file"
            || candidate.kind() == "program"
            || candidate.kind() == "root"
            || candidate.kind() == "block"
        {
            break;
        }
        current = candidate.parent();
    }
    false
}

fn visibility_for_node(node: Node<'_>, role: SymbolRole, source: &[u8]) -> SymbolVisibility {
    if role == SymbolRole::Reference {
        return SymbolVisibility::Unknown;
    }
    let declaration = context_snippet(node, source, &[]).to_ascii_lowercase();
    let starts_with = declaration.trim_start();
    if starts_with.starts_with("pub(")
        || starts_with.starts_with("pub ")
        || starts_with.starts_with("public ")
        || starts_with.starts_with("export ")
    {
        SymbolVisibility::Public
    } else if starts_with.starts_with("private ") || starts_with.starts_with("private\t") {
        SymbolVisibility::Private
    } else if starts_with.starts_with("protected ")
        || starts_with.starts_with("internal ")
        || starts_with.starts_with("protected\t")
        || starts_with.starts_with("internal\t")
    {
        SymbolVisibility::Internal
    } else {
        SymbolVisibility::Unknown
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
    let declaration = if is_import_declaration_kind(declaration.kind()) {
        nearest_import_statement(declaration).unwrap_or(declaration)
    } else {
        declaration
    };
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

fn is_import_declaration_kind(kind: &str) -> bool {
    matches!(
        kind,
        "import_specifier"
            | "import_clause"
            | "namespace_import"
            | "named_imports"
            | "import_declaration"
            | "import_statement"
            | "import_from_statement"
            | "use_declaration"
            | "using_directive"
    )
}

fn nearest_import_statement(node: Node<'_>) -> Option<Node<'_>> {
    let mut current = node.parent();
    while let Some(candidate) = current {
        if matches!(
            candidate.kind(),
            "import_statement"
                | "import_declaration"
                | "import_from_statement"
                | "use_declaration"
                | "using_directive"
                | "import_directive"
        ) {
            return Some(candidate);
        }
        current = candidate.parent();
    }
    None
}

fn compact_text(bytes: &[u8]) -> String {
    let mut output = String::new();
    for word in String::from_utf8_lossy(bytes).split_whitespace() {
        let separator = usize::from(!output.is_empty());
        if output.chars().count().saturating_add(separator) >= MAX_CONTEXT_CHARS {
            output.push('…');
            break;
        }
        if separator == 1 {
            output.push(' ');
        }
        let remaining = MAX_CONTEXT_CHARS.saturating_sub(output.chars().count());
        output.extend(word.chars().take(remaining));
        if output.chars().count() < MAX_CONTEXT_CHARS && word.chars().count() > remaining {
            output.push('…');
            break;
        }
    }
    output
}

fn text_for_node(node: Node<'_>, source: &[u8]) -> String {
    source
        .get(node.start_byte().min(source.len())..node.end_byte().min(source.len()))
        .map(|bytes| String::from_utf8_lossy(bytes).chars().take(256).collect())
        .unwrap_or_default()
}

fn collect_parse_findings(
    node: Node<'_>, source: &[u8], findings: &mut Vec<MapFinding>, max_depth: usize, max_findings: usize,
) -> bool {
    let mut stack = vec![(node, 0usize)];
    let mut truncated = false;
    while let Some((node, depth)) = stack.pop() {
        if depth > max_depth {
            truncated = true;
            if findings.len() < max_findings {
                findings.push(MapFinding {
                    kind: MapFindingKind::ParseError,
                    path: String::new(),
                    location: Some(SourceLocation::from(node)),
                    detail: format!(
                        "Syntax traversal exceeded the depth limit of {max_depth}; deeper nodes were omitted."
                    ),
                });
            }
            continue;
        }
        if (node.is_error() || node.is_missing()) && findings.len() < max_findings {
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
        let children = node.children(&mut cursor).collect::<Vec<_>>();
        for child in children.into_iter().rev() {
            stack.push((child, depth.saturating_add(1)));
        }
    }
    truncated
}

fn add_ambiguity_findings(edges: &[LexicalEdge], findings: &mut Vec<MapFinding>, max_findings: usize) {
    let mut groups = BTreeMap::<String, (String, String, usize, LexicalResolutionReason)>::new();
    for edge in edges.iter().filter(|edge| edge.ambiguous) {
        let entry = groups.entry(edge.candidate_group.clone()).or_insert_with(|| {
            (
                edge.source.clone(),
                edge.symbol.clone(),
                edge.candidates.len(),
                edge.resolution_reason,
            )
        });
        entry.2 = entry.2.max(edge.candidates.len());
    }
    for (group, (path, symbol, candidates, reason)) in
        groups.into_iter().take(max_findings.saturating_sub(findings.len()))
    {
        findings.push(MapFinding {
            kind: MapFindingKind::AmbiguousReference,
            path,
            location: None,
            detail: format!(
                "Lexical reference `{symbol}` has {candidates} deduplicated definition candidates ({}) in candidate group `{group}`; no type-resolved relationship is asserted.",
                reason.label(),
            ),
        });
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
    fn lexical_edges_require_explicit_same_file_or_import_evidence_and_group_ambiguity() {
        fn file(path: &str, language: SourceLanguage, parsed: ParsedSource) -> SourceFile {
            SourceFile {
                path: path.to_owned(),
                language,
                extension: path.rsplit('.').next().unwrap_or_default().to_owned(),
                worktree_state: WorktreeState::Tracked,
                status: parsed.status,
                symbols: parsed.symbols,
                limitations: parsed.limitations,
            }
        }

        let rust_caller = file(
            "src/caller.rs",
            SourceLanguage::Rust,
            parse_source(b"fn caller() { target(); }", &RUST_SUPPORT),
        );
        let rust_target = file(
            "src/target.rs",
            SourceLanguage::Rust,
            parse_source(b"pub fn target() {}", &RUST_SUPPORT),
        );
        assert!(build_lexical_edges(&[rust_caller, rust_target], 32, 32).is_empty());
        let rust_imported_caller = file(
            "src/imported.rs",
            SourceLanguage::Rust,
            parse_source(b"use crate::target::target; fn caller() { target(); }", &RUST_SUPPORT),
        );
        let rust_imported_edges = build_lexical_edges(
            &[
                rust_imported_caller,
                file(
                    "src/target.rs",
                    SourceLanguage::Rust,
                    parse_source(b"pub fn target() {}", &RUST_SUPPORT),
                ),
            ],
            32,
            32,
        );
        let rust_imported_edge = rust_imported_edges.first().expect("Rust import-aware edge");
        assert_eq!(
            rust_imported_edge.resolution_reason,
            LexicalResolutionReason::ImportedModule
        );
        assert_eq!(rust_imported_edge.confidence, ConfidenceTier::High);

        let javascript_caller = file(
            "src/caller.js",
            SourceLanguage::JavaScript,
            parse_source(
                br#"import { target } from "./target.js";
target();"#,
                &JAVASCRIPT_SUPPORT,
            ),
        );
        let javascript_target = file(
            "src/target.js",
            SourceLanguage::JavaScript,
            parse_source(b"export function target() {}", &JAVASCRIPT_SUPPORT),
        );
        let explicit_edges = build_lexical_edges(&[javascript_caller, javascript_target], 32, 32);
        let edge = explicit_edges.first().expect("import-aware edge");
        assert_eq!(edge.resolution_reason, LexicalResolutionReason::ImportedModule);
        assert_eq!(edge.confidence, ConfidenceTier::High);
        assert_eq!(edge.target_visibility, SymbolVisibility::Public);
        assert!(!edge.candidate_group.is_empty());

        let ambiguous_caller = file(
            "src/ambiguous.js",
            SourceLanguage::JavaScript,
            parse_source(
                br#"import { target } from "./unknown.js";
target();"#,
                &JAVASCRIPT_SUPPORT,
            ),
        );
        let ambiguous_one = file(
            "src/one.js",
            SourceLanguage::JavaScript,
            parse_source(b"export function target() {}", &JAVASCRIPT_SUPPORT),
        );
        let ambiguous_two = file(
            "src/two.js",
            SourceLanguage::JavaScript,
            parse_source(b"export function target() {}", &JAVASCRIPT_SUPPORT),
        );
        let ambiguous_files = [ambiguous_caller, ambiguous_one, ambiguous_two];
        let ambiguous_edges = build_lexical_edges(&ambiguous_files, 32, 32);
        assert_eq!(ambiguous_edges.len(), 2);
        assert!(ambiguous_edges.iter().all(|edge| edge.ambiguous));
        assert_eq!(ambiguous_edges[0].candidate_group, ambiguous_edges[1].candidate_group);
        assert!(
            build_lexical_edges(&ambiguous_files, 1, 32)
                .iter()
                .all(|edge| edge.candidates.len() <= 1)
        );
        assert!(build_lexical_edges(&ambiguous_files, 32, 1).len() <= 1);
        assert!(build_lexical_edges(&ambiguous_files, 32, 0).is_empty());
        let mut findings = Vec::new();
        add_ambiguity_findings(&ambiguous_edges, &mut findings, 32);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].detail.contains("deduplicated definition candidates"));
    }

    #[test]
    fn cache_file_paths_normalize_without_basename_matching_or_scope_leaks() {
        let repository = Path::new("/repo");
        assert_eq!(
            normalized_cache_file_path("src\\lib.rs", repository),
            Some("src/lib.rs".to_owned())
        );
        assert_eq!(
            normalized_cache_file_path("/repo/src/./lib.rs", repository),
            Some("src/lib.rs".to_owned())
        );
        assert_eq!(
            normalized_cache_file_path("lib.rs", repository),
            Some("lib.rs".to_owned())
        );
        assert_eq!(normalized_cache_file_path("../src/lib.rs", repository), None);
    }

    #[test]
    fn cache_pruning_is_count_bounded_and_path_deterministic() {
        let root = std::env::temp_dir().join(format!("codeplat-cache-prune-{}", std::process::id()));
        let repository = root.join("repositories").join("repo");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&repository).expect("create cache prune fixture");
        for index in 0..(CACHE_MAX_RECORDS_PER_REPOSITORY + 4) {
            fs::write(repository.join(format!("record-{index:03}.json")), b"{}").expect("write cache prune record");
        }

        let (removed, _) = prune_cache_directory(&root, &repository).expect("prune cache fixture");
        assert_eq!(removed, 4);
        let remaining = collect_cache_files(&root, &repository).expect("read pruned cache fixture");
        assert_eq!(remaining.len(), CACHE_MAX_RECORDS_PER_REPOSITORY);
        fs::remove_dir_all(root).expect("remove cache prune fixture");
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

    #[test]
    fn snippet_selection_respects_every_tiny_token_budget() {
        let source = b"pub fn a_very_long_name(value: usize) { let _ = value; }";
        let ParsedSource { symbols, .. } = parse_source(source, &RUST_SUPPORT);
        let file = SourceFile {
            path: "src/lib.rs".to_owned(),
            language: SourceLanguage::Rust,
            extension: "rs".to_owned(),
            worktree_state: WorktreeState::Tracked,
            status: FileAnalysisStatus::Complete,
            symbols,
            limitations: Vec::new(),
        };
        let ranking = vec![FileRank { path: file.path.clone(), score: 1_000_000, focus_matches: 0 }];
        let settings = MapSettings::default();
        for budget in 1..=64 {
            let selection = select_snippets(std::slice::from_ref(&file), &[], &ranking, budget, &settings);
            assert!(selection.estimated_tokens <= budget, "budget {budget}: {selection:?}");
            assert!(
                selection
                    .snippets
                    .iter()
                    .all(|snippet| snippet.estimated_tokens <= budget)
            );
        }
    }
}
