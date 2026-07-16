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
fn go_query_pack_extracts_packages_declarations_references_visibility_and_scopes() {
    let source = br#"
package service

import (
    "fmt"
    api "example.com/project/client"
)

type Identifier = string
type Box[T any] struct {
    Value T
    hidden int
}
type Runner interface {
    Run(Identifier) error
    hiddenRun()
}

const ExportedConstant = 1
const localConstant = 2
var ExportedVariable Identifier
var localVariable Identifier

func Build[T any](value T) *Box[T] {
    var LocalOnly Identifier
    LocalShort := value
    fmt.Println(value)
    helper()
    return &Box[T]{Value: value}
}

func helper() {}

func (box *Box[T]) Process(input Identifier) {
    api.Send(input)
    helper()
}
"#;
    let parsed = parse_source(source, &GO_SUPPORT);

    assert_eq!(parsed.status, FileAnalysisStatus::Complete, "{parsed:?}");
    assert!(parsed.findings.is_empty(), "{parsed:?}");
    for (name, kind) in [
        ("service", SymbolKind::Module),
        ("fmt", SymbolKind::Import),
        ("api", SymbolKind::Import),
        ("Identifier", SymbolKind::Type),
        ("Box", SymbolKind::Struct),
        ("Runner", SymbolKind::Interface),
        ("Value", SymbolKind::Field),
        ("Run", SymbolKind::Method),
        ("ExportedConstant", SymbolKind::Const),
        ("ExportedVariable", SymbolKind::Variable),
        ("Build", SymbolKind::Function),
        ("Process", SymbolKind::Method),
    ] {
        assert!(
            parsed
                .symbols
                .iter()
                .any(|symbol| { symbol.name == name && symbol.kind == kind && symbol.role == SymbolRole::Definition }),
            "missing {kind:?} definition {name}: {parsed:?}"
        );
    }

    let build = parsed
        .symbols
        .iter()
        .find(|symbol| symbol.name == "Build" && symbol.role == SymbolRole::Definition)
        .expect("Go function definition");
    assert_eq!(build.scope, vec!["service"]);
    assert_eq!(build.visibility, SymbolVisibility::Public);
    assert!(build.context.starts_with("func Build[T any]"));

    let process = parsed
        .symbols
        .iter()
        .find(|symbol| symbol.name == "Process" && symbol.role == SymbolRole::Definition)
        .expect("Go method definition");
    assert_eq!(process.scope, vec!["service", "Box"]);
    assert_eq!(process.visibility, SymbolVisibility::Public);

    let hidden = parsed
        .symbols
        .iter()
        .find(|symbol| symbol.name == "hidden" && symbol.role == SymbolRole::Definition)
        .expect("unexported Go field");
    assert_eq!(hidden.scope, vec!["service", "Box"]);
    assert_eq!(hidden.visibility, SymbolVisibility::Internal);
    let local = parsed
        .symbols
        .iter()
        .find(|symbol| symbol.name == "LocalOnly" && symbol.role == SymbolRole::Definition)
        .expect("function-local Go variable");
    assert_eq!(local.visibility, SymbolVisibility::Internal);
    let local_short = parsed
        .symbols
        .iter()
        .find(|symbol| symbol.name == "LocalShort" && symbol.role == SymbolRole::Definition)
        .expect("short Go variable declaration");
    assert_eq!(local_short.kind, SymbolKind::Variable);
    assert_eq!(local_short.visibility, SymbolVisibility::Internal);

    for (name, evidence) in [
        ("Println", SymbolEvidence::Call),
        ("Send", SymbolEvidence::Call),
        ("helper", SymbolEvidence::Call),
        ("Identifier", SymbolEvidence::TypeReference),
    ] {
        assert!(
            parsed.symbols.iter().any(|symbol| {
                symbol.name == name && symbol.role == SymbolRole::Reference && symbol.evidence == evidence
            }),
            "missing {evidence:?} reference {name}: {parsed:?}"
        );
    }
}

#[test]
fn malformed_go_is_partial_and_test_declarations_remain_available() {
    let valid = parse_source(
        b"package service\nfunc TestBuild(t *testing.T) { Build(1) }\n",
        &GO_SUPPORT,
    );
    assert_eq!(valid.status, FileAnalysisStatus::Complete, "{valid:?}");
    assert!(valid.symbols.iter().any(|symbol| {
        symbol.name == "TestBuild" && symbol.kind == SymbolKind::Function && symbol.role == SymbolRole::Definition
    }));

    let malformed = parse_source(b"package service\nfunc Broken( {\n", &GO_SUPPORT);
    assert_eq!(malformed.status, FileAnalysisStatus::Partial);
    assert!(
        malformed
            .findings
            .iter()
            .any(|finding| finding.kind == MapFindingKind::ParseError)
    );
    assert!(
        malformed
            .limitations
            .iter()
            .any(|limitation| limitation.contains("Go file"))
    );
}

#[test]
fn lua_query_pack_extracts_modules_declarations_references_and_scopes() {
    let source = br#"
local helper = require("app.helper")
local M = { version = 1 }
local local_value = 2
local declared_only
global_value = 3

local function build(value)
    local nested = value
    return helper.transform(nested)
end

function M.run(value)
    return build(value)
end

function M:render()
    return self:run(global_value)
end

M.assigned = function(value)
    return value
end

return {
    create = build,
    start = function() return M:render() end,
}
"#;
    let parsed = parse_source(source, &LUA_SUPPORT);

    assert_eq!(parsed.status, FileAnalysisStatus::Complete, "{parsed:?}");
    assert!(parsed.findings.is_empty(), "{parsed:?}");
    for (name, kind) in [
        ("app.helper", SymbolKind::Import),
        ("helper", SymbolKind::Variable),
        ("local_value", SymbolKind::Variable),
        ("declared_only", SymbolKind::Variable),
        ("global_value", SymbolKind::Variable),
        ("build", SymbolKind::Function),
        ("run", SymbolKind::Function),
        ("render", SymbolKind::Method),
        ("assigned", SymbolKind::Function),
        ("version", SymbolKind::Field),
        ("create", SymbolKind::Field),
        ("start", SymbolKind::Function),
    ] {
        assert!(
            parsed
                .symbols
                .iter()
                .any(|symbol| { symbol.name == name && symbol.kind == kind && symbol.role == SymbolRole::Definition }),
            "missing {kind:?} definition {name}: {parsed:?}"
        );
    }

    let build = parsed
        .symbols
        .iter()
        .find(|symbol| symbol.name == "build" && symbol.kind == SymbolKind::Function)
        .expect("local Lua function");
    assert_eq!(build.visibility, SymbolVisibility::Internal);
    let nested = parsed
        .symbols
        .iter()
        .find(|symbol| symbol.name == "nested" && symbol.role == SymbolRole::Definition)
        .expect("nested local variable");
    assert_eq!(nested.scope, vec!["build"]);
    let run = parsed
        .symbols
        .iter()
        .find(|symbol| symbol.name == "run" && symbol.role == SymbolRole::Definition)
        .expect("dot method definition");
    assert_eq!(run.scope, vec!["M"]);
    let render = parsed
        .symbols
        .iter()
        .find(|symbol| symbol.name == "render" && symbol.role == SymbolRole::Definition)
        .expect("colon method definition");
    assert_eq!(render.scope, vec!["M"]);
    for name in ["transform", "build", "run", "render"] {
        assert!(
            parsed
                .symbols
                .iter()
                .any(|symbol| symbol.name == name && symbol.role == SymbolRole::Reference),
            "missing reference {name}: {parsed:?}"
        );
    }
    assert!(
        parsed
            .limitations
            .iter()
            .any(|limitation| { limitation.contains("dynamic `require`") && limitation.contains("metatable") })
    );
}

#[test]
fn malformed_lua_is_partial_and_dynamic_require_is_not_import_evidence() {
    let dynamic = parse_source(
        b"local name = 'app.helper'\nlocal helper = require(name)\n",
        &LUA_SUPPORT,
    );
    assert_eq!(dynamic.status, FileAnalysisStatus::Complete, "{dynamic:?}");
    assert!(dynamic.symbols.iter().all(|symbol| symbol.kind != SymbolKind::Import));

    let malformed = parse_source(b"local function broken(\n  return { value = 1\n", &LUA_SUPPORT);
    assert_eq!(malformed.status, FileAnalysisStatus::Partial);
    assert!(
        malformed
            .findings
            .iter()
            .any(|finding| finding.kind == MapFindingKind::ParseError)
    );
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
    assert_eq!(
        support_for_path(Path::new("service_test.go")).unwrap().language,
        SourceLanguage::Go
    );
    assert_eq!(
        support_for_path(Path::new("module.lua")).unwrap().language,
        SourceLanguage::Lua
    );
    assert_eq!(
        support_for_path(Path::new("package.rockspec")).unwrap().language,
        SourceLanguage::Lua
    );
    assert_eq!(
        support_for_path(Path::new(".luacheckrc")).unwrap().language,
        SourceLanguage::Lua
    );
    assert_eq!(
        support_for_path(Path::new(".busted")).unwrap().language,
        SourceLanguage::Lua
    );
    assert_eq!(
        lua_support_for_entry_source(Path::new("bin/tool"), "#!/usr/bin/env lua\nprint('ok')")
            .unwrap()
            .language,
        SourceLanguage::Lua
    );
    assert_eq!(
        lua_support_for_entry_source(Path::new("scripts/tool"), "#!/usr/bin/luajit\nprint('ok')")
            .unwrap()
            .language,
        SourceLanguage::Lua
    );
    assert!(lua_support_for_entry_source(Path::new("bin/tool"), "#!/bin/sh\necho ok").is_none());
    assert!(lua_support_for_entry_source(Path::new("tool"), "#!/usr/bin/env lua\nprint('ok')").is_none());
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
            classifications: Vec::new(),
            classification_overridden: false,
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

    let go_same_package = build_lexical_edges(
        &[
            file(
                "service/caller.go",
                SourceLanguage::Go,
                parse_source(b"package service\nfunc caller() { Build() }", &GO_SUPPORT),
            ),
            file(
                "service/build.go",
                SourceLanguage::Go,
                parse_source(b"package service\nfunc Build() {}", &GO_SUPPORT),
            ),
        ],
        32,
        32,
    );
    let go_edge = go_same_package.first().expect("Go same-package edge");
    assert_eq!(go_edge.symbol, "Build");
    assert_eq!(go_edge.resolution_reason, LexicalResolutionReason::SameModule);
    assert_eq!(go_edge.confidence, ConfidenceTier::High);

    let go_different_directories = build_lexical_edges(
        &[
            file(
                "one/caller.go",
                SourceLanguage::Go,
                parse_source(b"package service\nfunc caller() { Build() }", &GO_SUPPORT),
            ),
            file(
                "two/build.go",
                SourceLanguage::Go,
                parse_source(b"package service\nfunc Build() {}", &GO_SUPPORT),
            ),
        ],
        32,
        32,
    );
    assert!(go_different_directories.is_empty());

    let go_imported = build_lexical_edges(
        &[
            file(
                "cmd/app/main.go",
                SourceLanguage::Go,
                parse_source(
                    b"package main\nimport client \"example.com/project/internal/client\"\nfunc main() { client.Build() }",
                    &GO_SUPPORT,
                ),
            ),
            file(
                "internal/client/build.go",
                SourceLanguage::Go,
                parse_source(b"package client\nfunc Build() {}", &GO_SUPPORT),
            ),
        ],
        32,
        32,
    );
    let imported_go_edge = go_imported.first().expect("Go import-path edge");
    assert_eq!(imported_go_edge.symbol, "Build");
    assert_eq!(
        imported_go_edge.resolution_reason,
        LexicalResolutionReason::ImportedModule
    );

    let lua_imported = build_lexical_edges(
        &[
            file(
                "app/main.lua",
                SourceLanguage::Lua,
                parse_source(
                    b"local helper = require('app.helper')\nreturn helper.build()",
                    &LUA_SUPPORT,
                ),
            ),
            file(
                "app/helper.lua",
                SourceLanguage::Lua,
                parse_source(b"local M = {}\nfunction M.build() end\nreturn M", &LUA_SUPPORT),
            ),
            file(
                "app/helper.js",
                SourceLanguage::JavaScript,
                parse_source(b"export function build() {}", &JAVASCRIPT_SUPPORT),
            ),
        ],
        32,
        32,
    );
    let lua_edge = lua_imported.first().expect("Lua literal-require edge");
    assert_eq!(lua_edge.source, "app/main.lua");
    assert_eq!(lua_edge.target, "app/helper.lua");
    assert_eq!(lua_edge.symbol, "build");
    assert_eq!(lua_edge.resolution_reason, LexicalResolutionReason::ImportedModule);
    assert!(lua_imported.iter().all(|edge| edge.target != "app/helper.js"));

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

    let dotted_javascript_import = build_lexical_edges(
        &[
            file(
                "src/caller.js",
                SourceLanguage::JavaScript,
                parse_source(
                    br#"import { target } from "./foo.bar.js";
target();"#,
                    &JAVASCRIPT_SUPPORT,
                ),
            ),
            file(
                "src/foo.bar.js",
                SourceLanguage::JavaScript,
                parse_source(b"export function target() {}", &JAVASCRIPT_SUPPORT),
            ),
            file(
                "src/foo/bar.js",
                SourceLanguage::JavaScript,
                parse_source(b"export function target() {}", &JAVASCRIPT_SUPPORT),
            ),
        ],
        32,
        32,
    );
    assert_eq!(dotted_javascript_import.len(), 1);
    assert_eq!(dotted_javascript_import[0].target, "src/foo.bar.js");

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
        classifications: Vec::new(),
        classification_overridden: false,
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

#[test]
fn classification_rules_are_deterministic_and_conservative() {
    let generated = classified_path("src/generated/parser.generated.rs");
    assert!(generated.iter().any(|classification| {
        classification.kind == SourceClassificationKind::Generated
            && classification.reason == "generated_directory:generated"
    }));
    assert!(generated.iter().any(|classification| {
        classification.kind == SourceClassificationKind::Generated && classification.reason == "generated_filename"
    }));
    assert!(
        classified_path("vendor/bundle.min.js")
            .iter()
            .any(|classification| { classification.kind == SourceClassificationKind::Vendor })
    );
    assert!(
        classified_path("assets/app.js.map")
            .iter()
            .any(|classification| { classification.kind == SourceClassificationKind::SourceMap })
    );
    assert!(classified_path("src/generated_parser.rs").is_empty());
}

#[test]
fn generated_markers_and_bounded_minification_are_content_evidence() {
    assert!(generated_marker(
        "// Code generated by fixture. DO NOT EDIT.\nfn generated() {}"
    ));
    assert!(!generated_marker("fn main() { println!(\"generated file\"); }"));

    let minified = format!("const value={};", "x".repeat(1_200));
    assert!(minified_heuristic("bundle.js", &minified));
    assert!(!minified_heuristic("maintained.rs", &minified));
    assert!(!minified_heuristic(
        "maintained.js",
        &format!("const value = {};\n", "x ".repeat(600))
    ));
}
