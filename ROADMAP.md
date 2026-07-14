---
title: "Setaryb: repository navigation CLI"
status: "ready"
---

## Objective

Build `setaryb` (Setāreyāb, Persian for _astrolabe_): a read-only Rust CLI that helps a
person or coding agent orient in a Git repository before reading arbitrary source files.

Its default command produces a concise, evidence-backed briefing in Markdown or a stable
JSON document. The briefing combines Git-history triage with a token-budgeted, task-sensitive
source map. Focused commands expose the same information without requiring callers to parse a
larger report.

## Users and Use Cases

- A developer entering an unfamiliar repository can run `setaryb` and see which paths to inspect
  first, why they matter, and what caveats apply.
- An agent can run `setaryb --json` or `setaryb map --json` and consume a versioned structural map
  without ANSI escapes or status chatter in stdout.
- A maintainer can examine churn, contributor concentration, fix-related path overlap, delivery activity,
  and firefighting language without shelling out to Git.
- A user can focus map ranking on an explicit task phrase or path, then retrieve a bounded outline of the
  most relevant symbols and source locations.

## Success Criteria

- `setaryb [PATH]` produces a read-only integrated briefing in Markdown; `--json` yields the equivalent
  structured document.
- The source map supports Rust, JavaScript, TypeScript, Python, Ruby, Java, and C# with tests for
  definitions, references, signatures, locations, and malformed input.
- Every requested map explicitly states unsupported or partial language analysis, lexical-reference
  ambiguity, scope, cache state, and history caveats rather than presenting inference as fact.
- Git-history output covers all five diagnostics from the source article: churn hotspots, contributor
  concentration, bug clusters and their churn overlap, monthly activity, and firefighting language.
- JSON has a documented `schema_version: 1` contract, zero ANSI/control sequences, and semantic
  compatibility fixtures.
- Markdown and JSON are rendered from one typed report model; Markdown is produced directly, never
  parsed as input.
- All target-repository operations are read-only. Cache data is stored only in the user cache
  directory and is optional via `--no-cache`.
- Black-box CLI fixture tests cover the output contract and pass for supported languages, cache
  modes, worktree states, ignores, and error cases.

## Current State

- The repository contains a Rust 2024 binary crate named `setaryb` with no dependencies and a
  `Hello, world!` entry point.
- `ROADMAP.md`, `TODO.md`, and the top-level README were initially empty.
- [Research notes](notes/README.md) capture the source material, Rust library boundaries, and
  the limits of Git-history and Tree-sitter-derived evidence.

## Product Contract

### Command surface

The exact help text may evolve, but these stable operations define v1:

```text
setaryb [OPTIONS] [PATH]
setaryb map [OPTIONS] [PATH]
setaryb history [OPTIONS] [PATH]
setaryb history <churn|contributors|bugs|activity|firefighting> [OPTIONS] [PATH]
```

- The default command is the integrated briefing. It does not hide a catch-all subcommand.
- `map` emits only repository-map findings and limitations.
- `history` emits all five history findings; its children emit one focused diagnostic.
- `PATH` defaults to the current directory. Setaryb discovers the enclosing Git repository and
  scopes analysis to the selected directory within that repository.
- `--format <markdown|json>` selects output, with Markdown as the default. `--json` is a
  standard explicit shorthand for `--format json`.
- `--focus <text>` and `--focus-path <path>` may be repeated on the default command and `map`.
  They are the only task-personalization inputs.
- `--map-tokens <n>` controls the maximum token budget for the compact source map; the initial
  default is 1,000 tokens.
- `--exclude <glob>` may be repeated to narrow a caller's analysis intentionally.
  Output records all supplied exclusions.
- `--no-cache` disables reading and writing analysis cache data. Cache refresh modes
  are `auto`, `always`, `files`, and `manual`; `files` requires explicit changed-file paths, while `manual`
  never silently refreshes a stale entry and labels its result accordingly.
- `--color <auto|always|never>` defaults to `auto`; `--no-color` is an alias for `--color never`.

### Output and stream contract

- The primary report is written to stdout. JSON is the machine contract; Markdown is the human- and model-readable report.
- Stdout is always plain JSON or plain Markdown. It never contains ANSI styling, progress, diagnostics, or prompts.
- Owo-colors may style help, warnings, progress, and errors on interactive stderr only. `auto` respects `NO_COLOR`,
  `TERM=dumb`, and the actual capability of each destination stream.
- Successful commands exit zero. Parsing, repository-discovery, input/access, and analysis failures have documented
  non-zero exit categories; errors explain the failed path and the next corrective action.
- JSON has `schema_version: 1`. Additive fields are allowed within schema version 1; removing, retyping, or changing
  the meaning of an existing field requires a schema-version change.
- Markdown communicates the same findings as JSON but its whitespace and ordering are not a public byte-for-byte API.

### Scope, ignores, and worktree state

- History analysis considers committed Git data and reports paths under the requested scope.
  It does not infer semantic quality from a history count.
- Source-map analysis includes every Git-tracked eligible source file in scope, including a tracked file
  that happens to match an ignore pattern.
- It also includes untracked, non-ignored eligible source files and labels them `untracked` in both formats.
- Ignored untracked files are omitted by default. Setaryb uses the `ignore` crate for traversal and Git-style
  matching, but records the effective policy rather than treating ignored content as universally irrelevant.
- Results identify the selected path, repository root, analyzed-path counts, omitted-path counts and reasons,
  and whether a file was tracked, modified, or untracked when that state affects interpretation.

## Functional Requirements

### Git-history briefing

Implement all five diagnostics using `gix` repository and revision APIs, never the system `git` executable:

| Finding        | Default analysis                                                                                                                         | Required caveats                                                                                 |
| -------------- | ---------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| Churn hotspots | Count committed changes per in-scope path for the preceding year.                                                                        | Absolute churn is not normalized by file size; active development is not automatically risky.    |
| Contributors   | Rank non-merge commit authors overall and for the previous six months. Surface high concentration and inactive historical concentration. | Squash merges can credit a merger rather than an author; commit count is only a knowledge proxy. |
| Bug clusters   | Count paths in commits matching case-insensitive `fix`, `bug`, or `broken`; show the intersection with churn hotspots.                   | Results depend on commit-message discipline and do not prove a defect rate.                      |
| Activity       | Aggregate all commits by author date month.                                                                                              | Cadence reflects team and release habits, not just repository health.                            |
| Firefighting   | List preceding-year commits matching `revert`, `hotfix`, `emergency`, or `rollback`.                                                     | No matches can mean stability or vague commit messages.                                          |

The common time-window and keyword defaults must be visible in output and overridable with explicit flags.
The report treats overlap as a prioritization signal, never a quality score or a judgment about contributors.

### Repository map

1. Enumerate candidate worktree paths according to the declared scope and ignore policy.
2. Detect the supported language from the path and select its embedded Tree-sitter grammar
   and versioned tag-query pack.
3. Parse source into a Tree-sitter syntax tree. Recover from parse errors where the grammar
   can still produce useful tags, and report partial analysis instead of inventing results.
4. Extract symbol definitions and references with language-specific queries. Record name, kind,
   role, path, line/column range, enclosing scope, and a compact declaration/signature snippet.
5. Build a directed lexical dependency graph: files are nodes, and a reference contributes an
   edge to files defining matching symbols. Preserve ambiguous candidates rather than claiming a type-resolved call graph.
6. Rank the graph with deterministic PageRank-style centrality plus explicit focus boosts for
   matching paths and identifiers.
   Downweight generic names and private-looking symbols only as a ranking heuristic, never as an omission rule.
7. Render the highest-ranked structural snippets until the token budget is reached.
   Use location-preserving ellipses and avoid emitting full file bodies by default.

Map output must give callers enough global API awareness to select the next full file to read,
while stating that it cannot replace source retrieval, type checking, import resolution, or runtime analysis.

### First-class language support

Each first-class language needs an embedded parser, versioned definition/reference query pack,
fixture coverage, and a documented support status:

- Rust
- JavaScript
- TypeScript
- Python
- Ruby
- Java
- C#

Unsupported extensions are reported as unsupported, not silently skipped from the report.
A grammar without a reliable reference query can still provide a clearly labelled definition-only
map only if the report makes that limitation explicit.

### Caching

- Cache parsed/tagged source-analysis inputs and reusable map-selection inputs in
  `$XDG_CACHE_HOME/setaryb`, falling back to `~/.cache/setaryb`.
- Never create or alter state in the target repository.
- Separate cache identity by canonical repository root, scope, language-query-pack version,
  tool/schema version, and source-content fingerprint.
  Cache validation must not make a stale entry appear current.
- `auto` validates and refreshes stale records; `always` reparses eligible inputs; `files`
  refreshes only caller-named changed paths; `manual` uses only available cache records and labels potentially stale output.
  `--no-cache` performs no cache I/O.

## Technical Plan

### Stack

- Rust edition 2024.
- `clap` 4 with derive support for typed root commands, subcommands, value enums, help, and validation.
- `serde` 1 and `serde_json` 1 for the typed report model and JSON schema.
- `owo-colors` 4 with its stream/capability support enabled for interactive stderr presentation.
- `gix` 0.85 with only the discovery, revision/object traversal, worktree-status, and trust-safe features
  required by the final implementation.
- `tree-sitter` 0.26 and mutually compatible official/upstream grammar crates for each first-class language.
  Grammar versions are pinned in `Cargo.lock` and upgraded only with query/fixture validation.
- `ignore` 0.4 for path traversal and explicit Git-style ignore matching.
- No Markdown parser dependency: Markdown is serialized directly from the report model. Add `comrak` or `pulldown-cmark`
  only if a future requirement introduces Markdown input or transformation.

### Architecture

Keep these layers independent:

1. **CLI:** parse commands and options into typed requests; choose a renderer only after analysis returns.
2. **Repository access:** use `gix` to discover the target repository and read references, commits, trees, and status. Never execute hooks, filters, external Git, or network operations.
3. **Worktree inventory:** use `ignore` with a declared policy; merge tracked-file knowledge with eligible untracked non-ignored files.
4. **History analysis:** produce typed evidence records and caveats from revision data.
5. **Language registry and tagging:** bind extensions to grammar/query packs, parse content, and return tags plus parse/support diagnostics.
6. **Map graph and ranking:** operate only on typed tags; keep PageRank and focus weighting deterministic and independently testable.
7. **Cache:** stores typed analysis intermediates, not presentation strings.
8. **Renderers:** produce schema-versioned JSON or stable-in-spirit Markdown from one report model.

### Trust, privacy, and read-only behavior

- Use gix's repository trust model; do not enable behavior that executes repository-controlled programs.
- Do not follow a path outside the requested repository scope through a symlink during worktree enumeration.
- Do not contact remotes, read chat/editor state, collect analytics, prompt for credentials, or mutate Git,
  the worktree, or project configuration.
- Use only explicit focus text and focus paths for task context.

## Testing Plan

### Test boundary

The acceptance boundary is the compiled CLI executing against fixture repositories.
Unit tests support individual parsers, graph routines, cache keys, and renderers, but
an implementation is not accepted based on unit tests alone.

### Required black-box fixtures

- A small Git-history fixture with commits that exercise every article diagnostic, including merge
  commits, vague messages, and an overlap between churn and bug-related changes.
- One source fixture per first-class language, with exported/public and private symbols, references,
  generic names, syntax errors, and declaration context.
- A mixed-language fixture with unsupported extensions and duplicate symbol names to prove lexical
  ambiguity is reported.
- A worktree fixture containing modified tracked files, untracked non-ignored source, ignored
  untracked artifacts, and a tracked path matching an ignore pattern.
- Cache fixtures for fresh, stale, manual, file-refresh, and disabled-cache modes using a temporary
  XDG cache directory.
- Stream fixtures proving JSON and Markdown stdout contain no ANSI escapes, while interactive stderr
  color policy remains independently testable.

### Required checks

```text
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

Each implementation ticket adds the narrowest relevant black-box case. JSON tests compare parsed semantic values and schema version;
Markdown snapshot tests guard readability and regressions without promising byte-for-byte compatibility to external callers.

## Project Structure

The implementation should evolve from the single binary into small Rust modules for CLI parsing, report schema, repository access,
worktree inventory, history analysis, language registry/query packs, map graph/ranking, cache, and renderers.
Keep language query files and test fixtures as first-class, versioned assets rather than hidden string constants.

## Code Style

- Prefer explicit data structures over formatted-string parsing and implicit global state.
- Keep filesystem, Git, parsing, ranking, caching, and rendering boundaries narrow and testable.
- Deterministically sort equivalent findings by documented tie-breakers before rendering.
- Treat a missing capability as typed output with an actionable explanation, not a panic or an empty success result.
- Keep comments for non-obvious invariants and trust/compatibility decisions; do not restate Rust syntax.

## Boundaries

- **Always:** preserve read-only target-repository behavior; render from the shared report model; use explicit focus inputs only;
  test at the CLI fixture boundary; document caveats with every inferred finding.
- **Ask first:** add a dependency beyond the selected stack; broaden supported-language promises;
  change JSON schema semantics; add any project configuration; relax repository/worktree safety limits; change cache location or retention.
- **Never:** invoke the system `git`, network, hooks, filters, or repository-controlled executables;
  write to the target repository; scrape chat/editor/agent context; color or log to report stdout;
  silently omit an analyzed-path category; claim lexical edges are semantic calls.

## Implementation Milestones

1. **Foundation:** establish typed CLI requests/results, schema versioning, JSON/Markdown renderers,
   stream/error behavior, fixtures, and command-level test harness.
2. **History orientation:** deliver the complete five-signal Git history briefing through the
   default command and focused history commands.
3. **Source-map core:** inventory current worktree state and produce a Rust structural map with
   locations, snippets, support diagnostics, and explicit limitations.
4. **First-class language coverage:** add and validate JavaScript, TypeScript, Python, Ruby, Java, and
   C# grammars and query packs.
5. **Relevant compact maps:** add lexical graph construction, deterministic rank/focus scoring,
   token-budget selection, cache modes, and cache observability.
6. **V1 hardening:** complete help and documentation, output/exit-code compatibility tests,
   performance and failure-mode checks, and release-quality verification.

## Deferred Milestones

### V1+ language coverage

Extend the same language-registry/query-pack contract to:

- F#
- Go
- Elixir
- C
- C++

Each language is added only with definition/reference query validation and black-box fixtures.
The architecture must accommodate this growth now; the support promise is deliberately staged
to keep first-class output trustworthy.

### Future repository-navigation depth

Broaden support tiers, query-pack distribution, and semantic-provider integration only after the
lexical map, history briefing, and JSON contract are stable. Such additions must retain explicit
uncertainty labels and must not turn the tool into a source editor or remote service by accident.

## Risks and Open Questions

- Tree-sitter grammar APIs and query node shapes evolve independently.
  Pin grammar versions and test every supported language before upgrades.
- Symbol-name matching creates ambiguous lexical edges, especially in object-oriented and dynamically typed languages.
  The map must expose candidates and limitations instead of false precision.
- Large repositories can make history walks, source reading, and graph ranking expensive.
  Cache correctness, bounded output, deterministic ordering, and progress/error behavior
  need measurement against realistic fixtures before release.
- Commit-message-based diagnostics are only as good as repository conventions.
  Empty or noisy results require caveats, not reinterpretation.
- Persistent user-local cache can contain source-derived metadata.
  Documentation must make its location, opt-out, and cleanup behavior clear.
- The exact grammar crate versions for the first-class language set must be selected as one compatible
  Tree-sitter set during implementation, then held by the lockfile.

## Settled Decisions

| Area                  | Decision                                                                                          |
| --------------------- | ------------------------------------------------------------------------------------------------- |
| Product focus         | Integrated default briefing plus focused `map` and `history` commands.                            |
| Formats               | Markdown by default and stable JSON; both render one typed result.                                |
| Markdown              | Output only; direct renderer, no Markdown parser dependency.                                      |
| Project name          | `setaryb` / Setāreyāb.                                                                            |
| First-class languages | Rust, JavaScript, TypeScript, Python, Ruby, Java, and C#.                                         |
| V1+ languages         | F#, Go, Elixir, C, and C++.                                                                       |
| Cache                 | On by default at the XDG user cache path, never project-local; `--no-cache` available.            |
| Scope                 | Never silently omit tracked paths; explicit caller exclusions; use `ignore` for traversal policy. |
| Worktree              | Include untracked non-ignored source in maps and label it; history is committed-data-only.        |
| Relevance             | Explicit `--focus` and `--focus-path` only; no session or prompt inspection.                      |
| JSON                  | Public, versioned schema from v1.                                                                 |
| Color                 | No color on report stdout; stream-aware Owo styling only on interactive stderr.                   |
| Verification          | Black-box CLI fixture tests are the primary acceptance boundary.                                  |

## Reference Material

- [Git history as a codebase triage tool](notes/git-history-triage.md)
- [Command-line interface design guidelines](notes/command-line-interface-guidelines.md)
- [Token-budgeted repository maps](notes/token-budgeted-repository-maps.md)
- [Rust building blocks for repository navigation](notes/rust-repository-navigation-building-blocks.md)
