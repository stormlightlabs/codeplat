---
title: Token-budgeted repository maps
sources:
  - https://aider.chat/docs/repomap.html
  - https://aider.chat/docs/languages.html
  - https://aider.chat/2023/10/22/repomap.html
  - https://github.com/paul-gauthier/aider/blob/main/aider/repomap.py
author: Paul Gauthier and Aider contributors
date: 2023-10-22
captured: 2026-07-14
tags:
  - code-navigation
  - tree-sitter
  - llm-context
---

## Summary

A repository map supplies broad, task-sensitive architectural context by rendering
a selected set of high-value symbol definitions from a dependency-ranked source graph
within a strict token budget.

## Source Boundary

- **Aider repository-map documentation and article:** The public model, benefits, token
  budget, and Tree-sitter language-support requirements.
- **Aider implementation reference:** Details reflected in the supplied source notes,
  including ranking personalization, definition/reference extraction, and caching;
  implementation details can evolve.

## Key Ideas

- **Breadth plus depth:** Full files provide local detail while a map exposes symbols elsewhere,
  so the user can request the next exact file rather than load everything.
- **Structural, not semantic:** The graph is built from lexical symbol definitions and references.
  It is not a type-resolved compiler call graph or a substitute for reading source.
- **Rank for the task:** A global graph becomes useful in a constrained context only after biasing
  it toward currently relevant files, paths, and identifiers.
- **Budget is behavior:** The renderer must select useful definitions and preserve declaration
  context until it reaches a measurable output budget.

## What a Repository Map Does

- Lists repository paths alongside selected classes, functions, methods, types, variables, and
  signatures.
- Uses Tree-sitter grammar queries such as `tags.scm` to extract symbol definitions and
  references when a language supports them.
- Connects source files through lexical references and ranks the resulting graph, commonly with
  PageRank-style centrality.
- Favors task-mentioned symbols and paths, downweights generic or private-looking identifiers,
  and emits snippets with enclosing declaration context.
- Caches parse/tag results based on file modification state and refreshes rendered selection
  according to a defined policy.

## How It Works

### Map pipeline

1. Enumerate eligible repository files and assign a language parser/query set.
2. Extract symbol tags; record definitions, references, signatures, line locations, and source paths.
3. Build a directed lexical dependency graph from reference-bearing files to definition-bearing files.
4. Apply a baseline graph ranking plus explicit task signals.
5. Render ranked declaration snippets until the token budget is reached, using ellipses to
   preserve structure without loading whole files.

### Language support boundary

Tree-sitter parsing alone is not enough for a useful map. Each language also needs reliable
symbol-query conventions. The Aider documentation states that adding repository-map support
requires a grammar's `tags.scm`; grammars without such queries need an alternative extraction
strategy or must be reported as unsupported.

### Limits and failure handling

Malformed source may still yield a parse tree, but queries can return partial or misleading tags.
Duplicate names, generic identifiers, dynamic dispatch, generated files, and unrecognized extensions reduce graph quality.
A map should state unsupported or partially supported languages rather than inventing structural facts.

## Claims & Evidence

### A compact map helps select the next full file to read

The Aider documentation says that a map exposes key definitions across the repository and lets
the agent identify which specific file needs more detail.

Confidence: medium-high; this is a documented product mechanism with an intuitive
information-retrieval rationale.

### Task-sensitive selection is required for large repositories

The documentation describes graph ranking and a map-token budget, while the supplied
implementation notes describe additional personalization for names and chat files.
Together they support selection rather than a fixed alphabetical outline.

Confidence: high for budgeted graph selection; medium for exact weighting because implementation
parameters change.

## Important Terms

| Term                     | Meaning                                                                                            |
| ------------------------ | -------------------------------------------------------------------------------------------------- |
| Repository map           | A compact structural index rendered for a human or model, not a complete code listing.             |
| Tag query                | A Tree-sitter query that identifies names, definitions, references, and their surrounding syntax.  |
| Lexical dependency graph | A graph inferred from matching symbol text and locations, without full type or runtime resolution. |
| Personalization          | Ranking adjustments derived from the current task's file paths and identifiers.                    |
| Token budget             | A maximum rendered-context size used to choose how much of the map to display.                     |

## Lessons To Reuse

- Keep extraction, ranking, and rendering independent so language support and ranking can evolve
  without rewriting output contracts.
- Preserve line locations and emitted-scope context; a symbol name without location is rarely
  actionable.
- Expose approximations plainly: reference edges are evidence for navigation, not proof of
  runtime calls.

## Questions for Review

- Why is a repository map not a replacement for source retrieval?
  - It exposes selected declarations and relationships, not complete behavior, control flow,
    documentation, or all uses.
- Why does Tree-sitter support not automatically mean map support?
  - The tool also needs high-quality queries that define what counts as a symbol and reference
    in that grammar.
- What role does the token budget play besides output truncation?
  - It makes ranking a product feature by deciding which information the viewer receives first.
- What makes a lexical dependency edge uncertain?
  - Textual names can be ambiguous and do not account for types, imports, dynamic dispatch, or
    runtime paths.

## Connections

- Related ideas: IDE symbol outlines, dependency analysis, PageRank, code search,
  retrieval-augmented context.
- Related sources: Tree-sitter queries and grammar repositories.
- Contradictions or tensions: broad coverage competes with precise detail; show a ranked outline
  and let users request full source.
- Useful applications: codebase onboarding, agent context construction, impact exploration,
  and identifying central interfaces.

## Open Questions

- Which language-query packs are sufficiently maintained to promise first-class support?
- How should a map represent symbols from files that cannot be parsed fully?
- Which task signals are valuable and safe without a conversational chat context?

## Takeaways

- The valuable artifact is a selected structural map, not an exhaustive index.
- Tree-sitter queries provide evidence for lexical relationships, not full semantic understanding.
- A transparent token budget, source location, and limitation reporting make the map trustworthy.
