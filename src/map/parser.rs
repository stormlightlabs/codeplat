use super::*;

pub struct LanguageAnalyzer<'a> {
    support: &'a LanguageSupport,
    parser: Parser,
    parser_error: Option<String>,
    definition_query: Option<Query>,
    definition_error: Option<String>,
    reference_query: Option<Query>,
    reference_error: Option<String>,
}

impl<'a> LanguageAnalyzer<'a> {
    pub fn new(support: &'a LanguageSupport) -> Self {
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

#[cfg(test)]
pub fn parse_source(source: &[u8], support: &LanguageSupport) -> ParsedSource {
    let mut analyzer = LanguageAnalyzer::new(support);
    parse_source_with_analyzer(
        source,
        &mut analyzer,
        &ReportLimits::for_profile(AnalysisProfile::Evidence),
    )
}

pub fn parse_source_with_analyzer(
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

pub fn capture_name(query: &Query, index: u32) -> &str {
    query
        .capture_names()
        .get(index as usize)
        .copied()
        .unwrap_or("reference.identifier")
}

pub fn symbol_from_capture(
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

pub fn evidence_for_node(node: Node<'_>, capture_name: &str, role: SymbolRole, kind: SymbolKind) -> SymbolEvidence {
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

pub fn is_call_like(node: Node<'_>) -> bool {
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

pub fn visibility_for_node(node: Node<'_>, role: SymbolRole, source: &[u8]) -> SymbolVisibility {
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

pub fn symbol_kind(capture_name: &str) -> SymbolKind {
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

pub fn declaration_node<'a>(node: Node<'a>, declaration_kinds: &[&str]) -> Node<'a> {
    let mut current = node;
    while let Some(parent) = current.parent() {
        if declaration_kinds.contains(&parent.kind()) {
            return parent;
        }
        current = parent;
    }
    node
}

pub fn scope_for_node(start: Option<Node<'_>>, source: &[u8], scope_kinds: &[&str]) -> Vec<String> {
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

pub fn context_snippet(node: Node<'_>, source: &[u8], declaration_kinds: &[&str]) -> String {
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

pub fn is_import_declaration_kind(kind: &str) -> bool {
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

pub fn nearest_import_statement(node: Node<'_>) -> Option<Node<'_>> {
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

pub fn compact_text(bytes: &[u8]) -> String {
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

pub fn text_for_node(node: Node<'_>, source: &[u8]) -> String {
    source
        .get(node.start_byte().min(source.len())..node.end_byte().min(source.len()))
        .map(|bytes| String::from_utf8_lossy(bytes).chars().take(256).collect())
        .unwrap_or_default()
}

pub fn collect_parse_findings(
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

pub fn add_ambiguity_findings(edges: &[LexicalEdge], findings: &mut Vec<MapFinding>, max_findings: usize) {
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

pub fn location_key(location: Option<&SourceLocation>) -> (usize, usize, usize, usize) {
    location.map_or((0, 0, 0, 0), |location| {
        (
            location.start.line,
            location.start.column,
            location.end.line,
            location.end.column,
        )
    })
}
