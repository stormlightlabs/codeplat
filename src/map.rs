mod analysis;
mod cache;
mod graph;
mod languages;
mod parser;
mod repository;

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use gix::bstr::ByteSlice;
use ignore::WalkBuilder;
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tree_sitter::{Node, Parser, Query, QueryCursor, StreamingIterator};

use crate::cli::ExitCategory;
use crate::{landmarks, security};
use crate::{report::*, utils};

#[cfg(test)]
use cache::{collect_cache_files, prune_cache_directory};

use cache::{CacheStats, CacheStore};
use graph::{build_lexical_edges, rank_files, select_snippets};
use languages::{
    c_sharp_language, extension_for_path, is_source_like_path, java_language, javascript_language, python_language,
    ruby_language, rust_language, support_for_path, supported_query_packs, tsx_language, typescript_language,
};
use parser::*;
use repository::*;

pub use analysis::analyze;
pub use cache::{CacheCommand, CacheControlReport, cache_control};

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
const MAX_CLASSIFICATION_SAMPLES: usize = 64;
const MINIFIED_HEURISTIC_MIN_BYTES: usize = 1_024;
const MINIFIED_HEURISTIC_MAX_BYTES: usize = 64 * 1_024;
const MINIFIED_HEURISTIC_MAX_WHITESPACE_PERCENT: usize = 2;
const MINIFIED_HEURISTIC_MIN_AVERAGE_LINE_BYTES: usize = 512;

type LanguageFactory = fn() -> tree_sitter::Language;

type Result<T> = std::result::Result<T, MapError>;

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

#[derive(Clone, Debug)]
pub struct MapSettings {
    pub excludes: Vec<String>,
    pub focuses: Vec<String>,
    pub focus_paths: Vec<String>,
    pub map_tokens: usize,
    pub cache_mode: CacheMode,
    pub cache_files: Vec<String>,
    pub recursive: bool,
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
            recursive: false,
            profile: AnalysisProfile::Compact,
        }
    }
}

#[derive(Clone, Debug)]
struct Candidate {
    state: WorktreeState,
    symlink: bool,
}

fn omission(path: String, reason: OmissionReason, detail: impl Into<String>) -> SourceOmission {
    SourceOmission {
        path,
        reason,
        detail: detail.into(),
        classifications: Vec::new(),
        classification_overridden: false,
    }
}

fn classified_omission(path: String, classifications: Vec<SourceClassification>, overridden: bool) -> SourceOmission {
    SourceOmission {
        path,
        reason: OmissionReason::Classified,
        detail: "The path was classified as generated, vendored, minified, or a source map and was excluded before parsing; use an exact `--focus-path` to inspect it when safe.".to_owned(),
        classifications,
        classification_overridden: overridden,
    }
}

fn record_classification(
    records: &mut Vec<SourceClassificationSample>, path: &str, classifications: &[SourceClassification],
    overridden: bool,
) {
    if classifications.is_empty() {
        return;
    }
    if let Some(record) = records.iter_mut().find(|record| record.path == path) {
        record.classifications.extend_from_slice(classifications);
        record.overridden |= overridden;
        record
            .classifications
            .sort_by(|left, right| left.kind.cmp(&right.kind).then_with(|| left.reason.cmp(&right.reason)));
        record.classifications.dedup_by(|right, left| right == left);
        return;
    }
    records.push(SourceClassificationSample {
        path: path.to_owned(),
        classifications: classifications.to_vec(),
        overridden,
    });
}

fn classification_summary(records: &mut Vec<SourceClassificationSample>) -> MapClassificationSummary {
    records.sort_by(|left, right| left.path.cmp(&right.path));
    records.dedup_by(|right, left| right.path == left.path);
    let total = records.len();
    let count = |kind: SourceClassificationKind| {
        records
            .iter()
            .filter(|record| {
                record
                    .classifications
                    .iter()
                    .any(|classification| classification.kind == kind)
            })
            .count()
    };
    let samples = records
        .iter()
        .take(MAX_CLASSIFICATION_SAMPLES)
        .cloned()
        .collect::<Vec<_>>();
    MapClassificationSummary {
        total,
        returned: samples.len(),
        truncated: samples.len() < total,
        reason: (samples.len() < total).then_some(TruncationReason::ProfileProjection),
        generated: count(SourceClassificationKind::Generated),
        vendor: count(SourceClassificationKind::Vendor),
        minified: count(SourceClassificationKind::Minified),
        source_map: count(SourceClassificationKind::SourceMap),
        samples,
    }
}

fn classified_path(path: &str) -> Vec<SourceClassification> {
    let mut classifications = Vec::new();
    for component in path.split('/').take_while(|component| !component.is_empty()) {
        let component = component.to_ascii_lowercase();
        let (kind, reason) = if matches!(
            component.as_str(),
            "target"
                | "dist"
                | "build"
                | "_build"
                | "out"
                | "coverage"
                | "generated"
                | "gen"
                | "obj"
                | "bin"
                | ".gradle"
                | ".next"
                | ".nuxt"
                | ".svelte-kit"
                | ".astro"
                | ".turbo"
                | ".vite"
                | ".dart_tool"
                | "tmp"
        ) {
            (
                SourceClassificationKind::Generated,
                format!("generated_directory:{component}"),
            )
        } else if matches!(
            component.as_str(),
            "vendor"
                | "node_modules"
                | ".pnpm-store"
                | ".venv"
                | "venv"
                | "bower_components"
                | "third_party"
                | "third-party"
                | "external"
                | "deps"
                | "vendor_modules"
                | "pods"
        ) {
            (
                SourceClassificationKind::Vendor,
                format!("vendor_directory:{component}"),
            )
        } else {
            continue;
        };
        classifications.push(SourceClassification { kind, reason });
    }

    let filename = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    let stem = filename.rsplit_once('.').map_or(filename.as_str(), |(stem, _)| stem);
    if filename.ends_with(".map") {
        classifications.push(SourceClassification {
            kind: SourceClassificationKind::SourceMap,
            reason: "source_map_filename".to_owned(),
        });
    }
    if filename.contains(".min.") || stem.ends_with(".min") {
        classifications.push(SourceClassification {
            kind: SourceClassificationKind::Minified,
            reason: "minified_filename".to_owned(),
        });
    }
    if matches!(
        stem,
        "generated" | "autogenerated" | "auto-generated" | "codegen" | "generated_code"
    ) || stem.ends_with(".generated")
        || stem.ends_with(".gen")
        || stem.ends_with("_pb")
        || stem.ends_with(".designer")
    {
        classifications.push(SourceClassification {
            kind: SourceClassificationKind::Generated,
            reason: "generated_filename".to_owned(),
        });
    }
    classifications.sort_by(|left, right| left.kind.cmp(&right.kind).then_with(|| left.reason.cmp(&right.reason)));
    classifications.dedup();
    classifications
}

fn classification_record_path(path: &str) -> String {
    let mut prefix = String::new();
    for component in path.split('/').filter(|component| !component.is_empty()) {
        if !prefix.is_empty() {
            prefix.push('/');
        }
        prefix.push_str(component);
        if !classified_path(&prefix).is_empty() {
            return prefix;
        }
    }
    path.to_owned()
}

fn focus_includes_path(path: &str, focus_paths: &[String]) -> bool {
    focus_paths.iter().any(|focus_path| {
        let focus_path = focus_path.trim().replace('\\', "/");
        let focus_path = focus_path.trim_start_matches("./");
        !focus_path.is_empty() && (path == focus_path || path.starts_with(&format!("{focus_path}/")))
    })
}

fn focus_descends_from(path: &str, focus_paths: &[String]) -> bool {
    focus_paths.iter().any(|focus_path| {
        let focus_path = focus_path.trim().replace('\\', "/");
        let focus_path = focus_path.trim_start_matches("./");
        !focus_path.is_empty() && (focus_path == path || focus_path.starts_with(&format!("{path}/")))
    })
}

fn comment_marker(line: &str) -> Option<&str> {
    let line = line.trim();
    ["//", "#", "/*", "*", "<!--", "--"]
        .iter()
        .find_map(|prefix| line.strip_prefix(prefix).map(str::trim))
}

fn generated_marker(source: &str) -> bool {
    source.lines().take(16).any(|line| {
        let Some(comment) = comment_marker(line) else {
            return false;
        };
        let comment = comment.to_ascii_lowercase();
        comment.contains("@generated")
            || comment.contains("generated file")
            || comment.contains("this file is generated")
            || comment.contains("this file was generated")
            || comment.contains("automatically generated")
            || (comment.contains("code generated") && comment.contains("do not edit"))
    })
}

fn minified_heuristic(path: &str, source: &str) -> bool {
    let extension = Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default();
    if !matches!(extension, "js" | "mjs" | "cjs" | "jsx" | "ts" | "mts" | "cts" | "tsx") {
        return false;
    }
    let bytes = source.len();
    if !(MINIFIED_HEURISTIC_MIN_BYTES..=MINIFIED_HEURISTIC_MAX_BYTES).contains(&bytes) {
        return false;
    }
    let lines = source.lines().collect::<Vec<_>>();
    let average_line_bytes = bytes / lines.len().max(1);
    let whitespace = source.bytes().filter(u8::is_ascii_whitespace).count();
    average_line_bytes >= MINIFIED_HEURISTIC_MIN_AVERAGE_LINE_BYTES
        && lines.len() <= 3
        && whitespace.saturating_mul(100) <= bytes.saturating_mul(MINIFIED_HEURISTIC_MAX_WHITESPACE_PERCENT)
}

fn source_classifications(path: &str, source: &str) -> Vec<SourceClassification> {
    let mut classifications = classified_path(path);
    if generated_marker(source) {
        classifications.push(SourceClassification {
            kind: SourceClassificationKind::Generated,
            reason: "generated_header_marker".to_owned(),
        });
    }
    if minified_heuristic(path, source) {
        classifications.push(SourceClassification {
            kind: SourceClassificationKind::Minified,
            reason: "bounded_minification_heuristic".to_owned(),
        });
    }
    classifications.sort_by(|left, right| left.kind.cmp(&right.kind).then_with(|| left.reason.cmp(&right.reason)));
    classifications.dedup();
    classifications
}

fn classification_override(path: &str, settings: &MapSettings) -> bool {
    focus_includes_path(path, &settings.focus_paths)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ParsedSource {
    symbols: Vec<SourceSymbol>,
    findings: Vec<MapFinding>,
    status: FileAnalysisStatus,
    limitations: Vec<String>,
}

#[derive(Clone)]
struct SnippetCandidate {
    path: String,
    language: SourceLanguage,
    symbol: SourceSymbol,
    score: u64,
}

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

/// Compile every embedded query against its grammar.
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
