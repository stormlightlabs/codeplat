---
title: "Codeplat: repository navigation CLI"
status: "in-progress"
updated: "2026-07-14"
---

## Objective

Build `codeplat` (code + _plat_, a plan or map of land): a read-only Rust CLI that helps a
person or coding agent orient in a Git repository before reading arbitrary source files.

Its default command produces a concise, evidence-backed briefing in Markdown or a stable
JSON document. The briefing combines Git-history triage with a token-budgeted, task-sensitive
source map. Focused commands expose the same information without requiring callers to parse a
larger report.

## Users and Use Cases

- A developer entering an unfamiliar repository can run `codeplat` and see which paths to inspect
  first, why they matter, and what caveats apply.
- An agent can run `codeplat --json` or `codeplat map --json` and consume a versioned structural map
  without ANSI escapes or status chatter in stdout.
- A maintainer can examine churn, contributor concentration, fix-related path overlap, delivery activity,
  and firefighting language without shelling out to Git.
- A user can focus map ranking on an explicit task phrase or path, then retrieve a bounded outline of the
  most relevant symbols and source locations.

## Success Criteria

- `codeplat [PATH]` produces a read-only integrated briefing in Markdown; `--json` yields the equivalent
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
- Hostile or malformed repository paths cannot escape the selected scope, and repository-controlled
  hooks, filters, commands, credentials, or network transports cannot execute during analysis.
- Default compact reports bound all data-dependent output, not only selected snippets. Every limit
  reports totals, returned counts, and truncation or omission reasons.
- Reports identify the analyzed revision, tool and query-pack versions, capture time, worktree state,
  cache freshness, and history completeness so two runs can be compared responsibly.

## Current State

- The Rust 2024 binary implements the focused `map` and `history` commands, both renderers,
  all five history diagnostics, cache modes, graph ranking, and Tree-sitter query packs for all
  seven first-class language families.
- Feature implementations for Tickets 1 through 14 exist, and the local formatting, test, Clippy,
  documentation, and release-build checks pass. The suite currently contains 44 tests; remaining release
  packaging and cross-platform gates are tracked by Ticket 17.
- The default `codeplat [PATH]` command now renders the promised integrated briefing: one typed report
  contains all five history diagnostics and the ranked, cache-aware source map. The remaining audit
  hardening and release criteria are tracked by Tickets 9 through 14.
- The **2026-07-14** audit found release-blocking trust-boundary, cache-validity,
  bounded-output, resource-limit, history-scope, and report-provenance gaps. Tickets 9 through 14
  define the stabilization work; they supersede checked acceptance criteria where the audit produced
  contradictory evidence.
- Ticket 11 now provides compact/evidence profiles, published work and output ceilings,
  iterative bounded traversal, collection totals/returned/truncation metadata, and
  operation-aware focused history scans. The compact self-map is approximately 13 KB
  in JSON; scale benchmarks remain a release-verification task.
- Ticket 13 now makes history envelopes scope-correct, applies committed `.mailmap` aliases while redacting
  email by default, records exact word-aware keyword matches, reports current-HEAD size-normalized churn,
  and labels rename continuity unavailable until bounded rename detection is implemented.
- Ticket 14 now uses language-scoped, import/module-aware lexical evidence with typed visibility, syntactic
  evidence, confidence/reason metadata, stable candidate groups, grouped ambiguity, per-language parser/query
  reuse, preaggregated incoming counts, and the bounded `explain` command. Bare cross-file and cross-language
  matches no longer affect centrality without explicit evidence.
- Ticket 15 now adds bounded, presence-based repository landmarks, detected workspace/package roots, grouped
  source recommendations, safe submodule/nested-repository boundaries, and explicit `--recursive` traversal.
- [Research notes](notes/README.md) capture the source material, Rust library boundaries, and
  the limits of Git-history and Tree-sitter-derived evidence.

## Baseline

The baseline is intentionally measured against Codeplat itself, not only tiny fixtures:

- `cargo fmt --check`, `cargo test --all-features`, `cargo clippy --all-targets --all-features
-- -D warnings`, and `cargo doc --no-deps` pass.
- The release binary is approximately 20 MiB while `gix` still uses its broad default feature set.
- The pre-Ticket-11 audit measured `codeplat map --no-cache --json` at 7,332,198 bytes for eight
  analyzed files. The compact profile now emits approximately 13 KB for the current self-map,
  and reports collection totals plus truncation reasons beside the bounded evidence.
- The current self-map contains 11,630 symbols, 2,212 ambiguity findings, and 795 lexical edges;
  repeated bare identifiers dominate useful architectural relationships.

These numbers are regression baselines, not acceptable targets. A release candidate must replace
them with explicit compact-output and latency ceilings measured on small, monorepo, deep-history,
malformed-tree, and ignored-vendor fixtures.

## Product Contract

### Command surface

The exact help text may evolve, but these stable operations define v1:

```text
codeplat [OPTIONS] [PATH]
codeplat map [OPTIONS] [PATH]
codeplat history [OPTIONS] [PATH]
codeplat history <churn|contributors|bugs|activity|firefighting> [OPTIONS] [PATH]
codeplat explain [OPTIONS] <PATH-OR-SYMBOL> [PATH]
codeplat cache <path|status|prune|clear>
codeplat capabilities [--json]
codeplat doctor [OPTIONS] [PATH]
```

- The default command is the integrated briefing. It does not hide a catch-all subcommand.
- `map` emits only repository-map findings and limitations.
- `history` emits all five history findings; its children emit one focused diagnostic.
- `PATH` defaults to the current directory. Codeplat discovers the enclosing Git repository and
  scopes analysis to the selected directory within that repository.
- `--format <markdown|json>` selects output, with Markdown as the default. `--json` is a
  standard explicit shorthand for `--format json`.
- `--focus <text>` and `--focus-path <path>` may be repeated on the default command and `map`.
  They are the only task-personalization inputs.
- `--map-tokens <n>` controls the maximum token budget for the compact source map; the initial
  default is 1,000 tokens.
- `--profile <compact|evidence>` defaults to bounded `compact`; exhaustive evidence is explicit and
  remains paginated or otherwise bounded.
- `--strict` applies the documented machine policy for stale, truncated, incomplete, unsupported, or
  partial results.
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
- Ignored untracked files are omitted by default. Codeplat uses the `ignore` crate for traversal and Git-style
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
Every focused operation obeys the selected path scope, including activity and envelope counts. If a future
signal is intentionally repository-wide, its section carries an explicit `scope_kind` rather than inheriting
the selected path label.

Contributor identities are canonicalized through `.mailmap` by default while preserving provenance about
the raw identities that were combined. Compact output does not expose email addresses unless the caller
requests them. Keyword evidence records the exact matched term and defaults to word-aware matching so
`fixture`, `prefix`, and `debug` do not become false `fix`/`bug` evidence.

After correctness is established, history can add size-normalized churn, rename-aware path continuity, and
declared ownership from `CODEOWNERS`. Each remains a separate evidence field beside absolute churn and raw
commit identity; no normalization silently replaces the article-derived baseline.

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

The graph must prefer evidence over fan-out. Candidate matching is constrained by language, import/module
context, and scope where the syntax supplies that information; same-file and cross-language bare-name
matches do not affect centrality by default. Every retained edge includes its resolution reason and confidence
tier. Candidate groups and repeated ambiguity diagnostics are deduplicated and capped before ranking.

Each language has a conformance corpus containing positive and negative tags, imports/aliases, overloads,
visibility, nesting, malformed input, and conventional extensionless entry files where applicable. Quality is
tracked with stable precision-oriented fixtures, not inferred from parser success alone.

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
  `$XDG_CACHE_HOME/codeplat`, falling back to `~/.cache/codeplat`.
- Never create or alter state in the target repository.
- Separate cache identity by canonical repository root, exact path, language-query-pack version,
  tool/schema version, and source-content fingerprint.
  Cache validation must not make a stale entry appear current.
- `auto` validates and refreshes stale records; `always` reparses eligible inputs; `files`
  refreshes only caller-named changed paths; `manual` uses only available cache records and labels potentially stale output.
  `--no-cache` performs no cache I/O.
- Cache writes are atomic and safe under concurrent readers. User-private permissions, bounded
  per-repository retention, and explicit `cache path`, `cache status`, `cache prune`, and `cache clear`
  operations make source-derived retention observable and controllable.
- Cache roots that resolve inside the analyzed repository are rejected. Parsed-file identity is
  independent of report scope, uses a reviewed collision-resistant content digest, and incorporates
  the exact grammar and query-pack content that produced the record.

## V1 Trustworthiness Release Gates

These are correctness requirements, not optional polish.

### Hostile-repository containment

**Implementation status (2026-07-15):** The current boundary is enforced by `src/security.rs`: Git paths
are validated from bytes, repository opening uses isolated restrictive gix options, status comparison
avoids the filter-aware status pipeline, worktree reads use Unix descriptor-relative no-follow traversal,
and cache roots/writes are checked against the canonical repository root. Non-Unix targets retain the
component reparse/symlink checks but use a weaker standard-library race fallback; cross-platform fixture
execution remains part of the release gate.

- Treat tree, index, status, and walk paths as untrusted byte strings. Accept only non-empty relative
  paths made of normal components; reject absolute paths, `.` and `..`, platform separator tricks,
  NULs, and lossy-decoding collisions before joining a worktree path.
- Do not follow a symlink or reparse point in any path component. Reads must remain beneath both the
  repository root and selected scope even if the worktree changes between inventory and open.
- Open `gix` repositories with an explicit hostile-input policy. Analysis must not execute hooks,
  clean/smudge/process filters, credential helpers, editors, pagers, shell commands, or network
  transports, including for a same-owner repository that Gitoxide would otherwise fully trust.
- Malicious-tree, intermediate-symlink, race, filter-sentinel, non-UTF-8, and Windows path fixtures
  are release tests. A rejected path becomes a typed safety diagnostic and is never read or cached.

### Cache validity

- Cache mode behavior is a tested state machine. In `auto`, a content-fingerprint miss parses the
  current bytes and reports a refresh; in `manual`, it may use the newest available record only while
  visibly marking it stale; in `files`, only normalized, exactly matched caller paths are refreshed.
- Unmatched `--cache-file` arguments and cache-unavailable files are reported explicitly. A file with
  no usable parsed record is not counted as analyzed.

### Bounded work and bounded reports

Ticket 11 implementation status (2026-07-15): compact/evidence profiles and
bounded collection metadata are implemented. Focused resource fixtures are in
the integration suite; scale benchmarks remain release-verification work.

- The default profile is compact. It emits selected structural evidence plus bounded summaries;
  exhaustive symbols, edges, findings, and commit evidence require an explicit evidence profile or
  pagination. Repeated ambiguity is grouped, not repeated per identifier occurrence.
- `--map-tokens` accounts for every data-dependent field in the compact map. The fixed envelope is
  documented and bounded, and the estimated-token total never exceeds the requested budget.
- Traversal prunes `.git`, ignored dependency/build/vendor directories, and nested repositories before
  descent. Hidden files are not treated as ignored merely because their names begin with a dot.
- File bytes, files, syntax depth, symbols, edge candidates, findings, commits, history evidence,
  elapsed work, and emitted output all have measured limits. Hitting a limit returns a useful partial
  report with `total`, `returned`, `truncated`, and reason metadata rather than hanging, exhausting
  memory, overflowing the stack, or silently dropping evidence.
- Focused history operations stream only the fields and diffs they require. A contributor or activity
  request must not recursively diff every reachable commit tree when paths are unnecessary.
- Potentially long scans provide concise progress on interactive stderr only, never report stdout, and
  honor interruption promptly without leaving a partial cache record. Non-interactive runs stay quiet.

### Reproducible and enforceable reports

Ticket 12 implementation status (2026-07-16): typed report provenance and quality state,
history completeness classification, strict policy, capabilities/doctor inspection, the
committed `schema/v1/codeplat.json` contract with `schema/v1/golden/` corpus, and a CI
compatibility gate are implemented.

- Every JSON report includes the Codeplat version, effective command/options, repository identity,
  resolved HEAD reference and object ID, capture/reference time, worktree snapshot state, grammar and
  query-pack versions, cache status, observed history range, and shallow/partial-history status.
- A committed JSON Schema and golden compatibility corpus define schema version 1. Schema changes are
  checked in CI; a numeric `schema_version` field alone is not a compatibility contract.
- Byte-native repository paths remain distinct internally. JSON uses one documented reversible
  representation; Markdown visibly escapes paths that cannot be represented as ordinary UTF-8.
- A strict policy lets automation reject stale, truncated, incomplete, unsupported, or partial reports
  through documented exit categories without scraping prose. Default mode still returns useful partial
  evidence and typed limitations.

## High-Value Navigation Extensions

Explainability and capability inspection complete the v1 trust contract. Landmarks and comparison follow
the v1 release. All are deliberately narrow additions to repository orientation, not a move toward an
editor, code generator, or semantic compiler.

### Repository landmarks

**Implementation status (2026-07-16):** The integrated briefing now emits bounded, typed landmarks and
project-root groups from the shared map scope. It identifies files a human or agent commonly
needs before source traversal: README and contributor instructions, `AGENTS.md`, manifests and lockfiles,
workspace/package roots, build and task entry points, test roots, CI configuration, ownership files, licenses,
submodules, and nested repositories. Each landmark includes a stable kind, detection reason, worktree state,
and project-root association; unknown or conflicting project-root roles remain `unknown`. Monorepos are grouped
by detected project root so the briefing exposes topology before symbol detail. Nested repositories and checked-out
submodules remain boundaries unless `--recursive` is explicitly supplied.

### Explainable recommendations

Add `codeplat explain <PATH-OR-SYMBOL>` in Markdown and JSON. It decomposes a recommendation into bounded,
typed evidence: focus matches, history overlap, graph edges, landmark role, ranking contributions, ambiguity,
and omitted alternatives. This is an explanation of Codeplat's heuristic, not a claim about program semantics.

Add `codeplat capabilities --json` and `codeplat doctor` so automation and people can inspect supported
languages, query-pack versions, schema versions, cache location/health, active safety policy, and resource
limits without analyzing a repository.

### Revision comparison

Add `codeplat compare --base <REV> [--head <REV|worktree>] [PATH]` after report provenance is stable. It reports
bounded changes to landmarks, public definitions, dependency evidence, hotspots, ownership concentration, and
recommended next reads. Comparison uses the same uncertainty labels and does not ingest Markdown as data.

## Technical Plan

### Stack

- Rust edition 2024.
- `clap` 4 with derive support for typed root commands, subcommands, value enums, help, and validation.
- `serde` 1 and `serde_json` 1 for the typed report model and JSON schema.
- `thiserror` 2 for typed internal errors, and `anyhow` 1 only at the top-level invocation
  boundary for contextual error propagation.
- `owo-colors` 4 with its stream/capability support enabled for interactive stderr presentation.
- `gix` 0.85 with only the discovery, revision/object traversal, worktree-status, and trust-safe features
  required by the final implementation. Disable default features and review the resulting feature tree;
  command, credential, transport, archive, worktree-mutation, and external-filter capabilities are not
  accepted merely because Codeplat does not intentionally call them.
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

Before adding more languages, split the current catch-all map and report modules along these existing
responsibilities: inventory/path safety, language/query registry, graph/ranking, selection/budgets, cache,
report schema, and renderers. This is a behavior-preserving maintainability step, not a new abstraction layer;
each extracted module needs a narrow typed boundary and the current black-box tests must remain authoritative.

### Trust, privacy, and read-only behavior

- Apply gix's repository trust model plus Codeplat's stricter hostile-input policy; same ownership does not
  authorize repository-controlled programs or paths outside the selected scope.
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
- Hostile-input fixtures with `..` tree entries, non-UTF-8 names, intermediate symlinks/reparse points,
  race attempts, nested repositories, and configured external Git filters whose sentinel must never run.
- Scale fixtures with large ignored trees, oversized and deeply nested syntax files, high symbol ambiguity,
  and 10,000-plus commits. Tests assert bounded output, work, memory-sensitive counts, and typed truncation.
- Compatibility fixtures validated against the committed JSON Schema, including older schema-version-1
  examples that must continue to deserialize with the same meaning.

### Required checks

```text
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo doc --no-deps
cargo package
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

1. **Foundation (implemented):** establish typed CLI requests/results, schema versioning, JSON/Markdown renderers,
   stream/error behavior, fixtures, and command-level test harness.
2. **History orientation (implemented, fixes pending):** deliver the complete five-signal Git history briefing through the
   default command and focused history commands.
3. **Source-map core (implemented, fixes pending):** inventory current worktree state and produce a Rust structural map with
   locations, snippets, support diagnostics, and explicit limitations.
4. **First-class language coverage (implemented, conformance expansion pending):** add and validate JavaScript, TypeScript, Python, Ruby, Java, and
   C# grammars and query packs.
5. **Relevant compact maps (implemented mechanically, quality gate failed):** add lexical graph construction, deterministic rank/focus scoring,
   token-budget selection, cache modes, and cache observability.
6. **Integrated briefing (implemented):** render the shared history, map, limitations, provenance, and
   ranked next-read evidence through the default command.
7. **Trust and resource hardening:** close hostile-path/filter execution, stale cache, scope, bounded-work,
   bounded-output, graph-signal, and machine-contract tickets with fixtures.
8. **Distributable v1:** complete help and documentation, cross-platform output/exit compatibility,
   minimal dependency features, package metadata/licensing, CI, performance gates, and release artifacts.
9. **Navigation depth:** add explainability and revision comparison after the implemented landmark topology;
   evaluate each addition against compact-output and simplicity budgets before starting the next.

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
  Cache correctness, operation-aware streaming, bounded output and evidence, deterministic ordering,
  and partial-result behavior require measured ceilings before release.
- A same-owner repository is still untrusted input. Gitoxide's trust classification alone does not prove
  that status, filters, configuration, or worktree access cannot execute repository-controlled programs.
- Git tree and index paths are byte strings, not trusted platform paths. Lossy conversion or unchecked joins
  can collapse identities or escape the repository even when a normal checkout would reject the tree.
- A token-budgeted snippet selection is not a compact report if symbols, edges, omissions, and diagnostics
  remain unbounded around it. Budget enforcement must be measured at the rendered contract boundary.
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
| Project name          | `codeplat`.                                                                                         |
| First-class languages | Rust, JavaScript, TypeScript, Python, Ruby, Java, and C#.                                         |
| V1+ languages         | F#, Go, Elixir, C, and C++.                                                                       |
| Cache                 | On by default at the XDG user cache path, never project-local; `--no-cache` available.            |
| Scope                 | Never silently omit tracked paths; explicit caller exclusions; use `ignore` for traversal policy. |
| Worktree              | Include untracked non-ignored source in maps and label it; history is committed-data-only.        |
| Relevance             | Explicit `--focus` and `--focus-path` only; no session or prompt inspection.                      |
| JSON                  | Public, versioned schema from v1, backed by a normative JSON Schema and compatibility corpus.     |
| Color                 | No color on report stdout; stream-aware Owo styling only on interactive stderr.                   |
| Verification          | Black-box CLI fixture tests are the primary acceptance boundary.                                  |

## Reference Material

- [Git history as a codebase triage tool](notes/git-history-triage.md)
- [Command-line interface design guidelines](notes/command-line-interface-guidelines.md)
- [Token-budgeted repository maps](notes/token-budgeted-repository-maps.md)
- [Rust building blocks for repository navigation](notes/rust-repository-navigation-building-blocks.md)
