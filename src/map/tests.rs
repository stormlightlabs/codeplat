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
