---
title: "Codeplat v1"
status: "in-progress"
updated: "2026-07-16"
---

## Objective

Codeplat is a read-only CLI that tells a person or coding agent what to read first in an unfamiliar Git
repository, why those paths matter, and what evidence is incomplete.

The V1 default must feel like an orientation briefing rather than a dump of analysis results. It combines a
short, evidence-backed reading plan with a few history observations. Focused commands and the evidence profile
retain detailed diagnostics for callers that need them.

## Users and Use Cases

- A developer entering an unfamiliar repository can run `codeplat` and get a useful first reading sequence
  without already knowing its language or layout.
- A coding agent can consume `codeplat --json` as a versioned, deterministic report and decide whether the
  evidence is sufficient for its task.
- A maintainer can inspect focused map and history evidence without invoking the system Git executable or
  allowing repository-controlled programs to run.
- A monorepo user can see the major project roots and receive recommendations that cover more than one relevant
  package instead of collapsing the repository into a single global winner.

## V1 Success Criteria

- The default Markdown briefing leads with 5–10 unique recommended paths when that much evidence exists,
  grouped by purpose and ordered into a practical reading sequence.
- Every recommendation states a concise reason grounded in typed landmark, source-map, focus, graph, or history
  evidence. A ranking score alone is not an explanation.
- The default history summary contains 3–5 useful observations rather than exhaustive path and commit lists.
  Detailed evidence remains available through `history`, focused history subcommands, JSON, and the evidence
  profile.
- Rust, JavaScript, TypeScript, Python, Ruby, Java, C#, Go, Lua, and Zig have first-class parser/query support,
  conformance fixtures, provenance, and honest partial-analysis behavior.
- Generated, vendored, and minified source does not consume the default reading plan or turn an otherwise useful
  report partial. Deterministic exclusions remain visible and can be overridden explicitly where safe.
- Expected compact sampling is distinguishable from actionable degradation. Collection summaries still expose
  totals and truncation, while top-level quality and `--strict` identify stale, incomplete, unsafe, unsupported,
  resource-limited, or relevant partial evidence that can change whether the report is usable.
- Small projects, Codeplat, supported mixed-language repositories, monorepos, unsupported-language fixtures, and
  scale fixtures pass both automated contract checks and a manual usefulness review.
- The CLI remains read-only, bounded, deterministic, quiet on broken pipes, safe against hostile repository
  paths/configuration, and distributable on Linux, macOS, and Windows at the declared MSRV.

## Current State

The Rust 2024 implementation already provides:

- an integrated Markdown/JSON briefing plus focused `map`, `history`, `explain`, `cache`, `capabilities`, and
  `doctor` commands;
- all five history evidence families, bounded compact/evidence profiles, typed provenance, schema-version-1
  compatibility fixtures, concise typed history observations, repository landmarks, project roots, cache controls,
  and explainable lexical edges;
- first-class Rust, JavaScript/JSX, TypeScript/TSX, Python, Ruby, Java, C#, and Go query packs;
- hostile-path and external-filter defenses, no-follow worktree reads, restrictive `gix` features, bounded work
  and output, package metadata, generated completions/man pages, and cross-platform release CI configuration.

A release-binary sweep across 53 first-party repositories completed without process failures or stderr output,
and identified product-usability work that is being addressed incrementally:

- the default Markdown now places the reading plan before up to five concise history observations; detailed history
  remains available through focused commands, JSON, and the evidence profile;
- only 41 ranked entries were returned across 53 reports, so many repositories received one recommendation or
  none;
- compact projection is now separated from actionable quality: hard limits, incomplete history, unsafe paths, and
  relevant unsupported/partial source drive strict policy, while unrelated omissions remain discoverable;
- Lua and Zig recur in the real project corpus but currently receive no structural map;
- minified or generated files are now classified before compact parsing, with bounded evidence samples and safe
  explicit overrides for focused inspection.

The detailed sweep evidence lives under `.sandbox/reports/`. It is development evidence, not a portable test
fixture or a promise about private repositories.

## Product Contract

### Command Surface

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

- `PATH` defaults to the current directory and must resolve within the discovered worktree.
- Markdown is the human default. `--json` is shorthand for `--format json` and remains the machine contract.
- `--focus` and `--focus-path` are the only task-personalization inputs and may be repeated.
- `--profile compact|evidence` defaults to `compact`. Both profiles remain bounded.
- `--map-tokens`, cache controls, exclusions, recursive traversal, color policy, and exit categories retain their
  documented behavior.
- `--strict` rejects actionable report degradation, not ordinary compact projection by itself.

### Default Briefing

The default Markdown order is:

1. **Repository overview:** detected project roots, primary supported languages, worktree state, and the most
   important instructions/manifests.
2. **Reading plan:** 5–10 paths when available, in a deliberate sequence and grouped under applicable purposes:
   `start_here`, `architecture`, `runtime`, `tests`, and `supporting_context`.
3. **History observations:** 3–5 bounded statements that identify evidence worth considering without converting
   heuristics into quality judgments.
4. **Limitations:** only limitations that materially affect the briefing, followed by a concise pointer to
   focused commands or `--profile evidence`.

Categories with no evidence are omitted. Recommendations are unique across categories. A monorepo reading plan
balances project-root coverage with relevance; it does not promise equal representation for unrelated roots.
When fewer than five useful paths exist, the report says why rather than padding the plan.

The default report must not print exhaustive churn tables, contributor tables, commit lists, full symbol lists,
all omissions, or all parser diagnostics. Focused commands preserve those workflows.

### Reading-Plan Evidence

Recommendations combine existing typed evidence rather than adding framework guesses:

- landmark priority: instructions, README/contributor guidance, manifests, entry points, tests, CI, and ownership;
- source evidence: public/visible definitions, qualified lexical edges, centrality, explicit focus matches, and
  declaration snippets;
- topology: detected project-root membership and nested-repository/submodule boundaries;
- history overlap: bounded churn, contributor, and fix-related evidence used only as a supporting signal.

Each recommendation records its purpose, ordinal, path, project root, concise reason, evidence kinds, confidence,
and any relevant limitation. The reason must explain the recommendation without requiring a caller to reverse
engineer an opaque score.

### History Observations

The five existing history families remain authoritative evidence inputs:

- churn hotspots;
- contributor concentration;
- bug-keyword paths and churn overlap;
- monthly activity;
- firefighting-keyword commits.

Default observations are selected for distinctness and actionability. They retain the existing caveats about
commit-message discipline, squash merges, exact-path rename continuity, current-HEAD normalization, and the fact
that activity is not a quality score. Empty or noisy evidence is summarized honestly and does not fill a quota.

### Quality and Strictness

Collection summaries always retain `total`, `returned`, `truncated`, and a reason. Top-level quality answers a
different question: whether the requested briefing may be unsafe or materially misleading.

- Expected compact projection is recorded at the collection level and does not alone make the report degraded.
- Resource ceilings, elapsed-work interruption, missing Git objects, stale manual cache, unsafe paths, unsupported
  relevant source, and partial analysis of recommended/focused files are actionable quality conditions.
- Partial or unsupported files outside the selected reading plan remain visible in totals/samples but do not
  automatically poison an otherwise useful compact briefing.
- `--strict` follows the actionable quality result and emits the typed report before returning the documented
  analysis-failure exit status.

Changes are additive within schema version 1 only when old fields retain their meaning. Any semantic reuse or
retyping requires a schema-version change and updated compatibility corpus.

### Generated, Vendored, and Minified Source

Default compact analysis applies deterministic, explainable classification before parsing:

- conventional generated/vendor/dependency directories are pruned;
- conventional generated filenames, generated-file markers, source maps, and `.min.*` files are classified;
- very low-whitespace/high-line-length minified text may be classified only by a documented bounded heuristic;
- tracked status never grants generated/vendor content priority over maintained source.

Compact mode records typed counts and bounded samples but does not parse or recommend classified files. An exact
focus path may include them when the file remains within all safety and resource limits. A
caller-supplied focus never overrides unsafe-path, binary, or hard resource protections.

### First-Class Language Support

V1 first-class languages are:

- Rust
- JavaScript and JSX
- TypeScript and TSX
- Python
- Ruby
- Java
- C#
- Go
- Lua
- Zig

Each language needs an upstream Tree-sitter grammar, embedded versioned definition/reference queries, stable
extension/filename registration, visibility and import/module evidence where the grammar exposes it, malformed
input behavior, and black-box Markdown/JSON fixtures. A definition-only limitation is permitted only when it is
explicitly represented and does not create false lexical edges.

Go coverage includes packages, imports/aliases, functions, methods, types, interfaces, fields, and common test
files. Lua coverage includes local/global functions, tables, methods, assignments, `require` evidence, and common
module layouts. Zig coverage includes containers, functions, declarations, tests, imports, and public visibility.

### Trust, Scope, and Cache

- Target repositories remain read-only. Codeplat never invokes system Git, hooks, filters, credentials, editors,
  pagers, repository commands, or network transports.
- Tree, index, walk, and cache paths remain untrusted until validated. Worktree reads do not follow symlinks or
  reparse points and stay beneath the repository and selected scope.
- Cache data lives only in the user cache directory, is keyed by repository/path/content/query/tool identity,
  uses private permissions and atomic writes, and remains controllable through cache commands.
- All file, byte, syntax-depth, symbol, candidate, edge, finding, commit, elapsed-time, and output ceilings remain
  enforced for every profile.

## Technical Plan

### Stack and Architecture

- Rust edition 2024 with MSRV 1.85.
- Clap for the typed CLI; serde/serde_json for the shared report model; Tree-sitter grammar crates for language
  analysis; `gix` with reviewed minimal features for repository data; `ignore` for bounded traversal.
- Preserve the current boundaries: CLI request parsing, security/repository access, history, landmarks/topology,
  language/query registry, cache, lexical graph/ranking, report schema, and rendering.
- Build the reading plan from typed report intermediates. Do not parse rendered Markdown or introduce a second
  analysis path for human output.
- Add dependencies only for reviewed upstream grammar support or when the standard library and current stack are
  insufficient.

### Testing Boundary

The highest stable boundary is the compiled CLI running against fixture repositories. Unit tests support query
captures, classification heuristics, ranking, selection, and rendering, but do not replace black-box assertions.

Required coverage includes:

- semantic Markdown snapshots for a small project, Codeplat, and a mixed-language monorepo;
- JSON assertions for reading-plan order, purpose, reasons, evidence, project-root coverage, quality, and schema
  compatibility;
- one conformance fixture per language plus supported mixed-language fixtures;
- generated, vendored, minified, malformed, high-ambiguity, ignored-tree, hostile-path, and deep-history fixtures;
- focused-path overrides and evidence-profile behavior for classified generated/vendor files;
- repeated-run determinism, no ANSI on stdout, empty non-interactive stderr, documented exits, and quiet broken
  pipes;
- benchmarks enforcing documented latency and output ceilings for Codeplat, a large ignored tree,
  high-ambiguity sources, and 10k/100k-commit histories.

The real-project sweep is repeated manually before V1 release. Private project contents or paths are not committed
as fixtures; only aggregate outcomes and reproducible public/synthetic regressions may enter the repository.

### Required Commands

```text
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps
cargo package --locked
cargo release-assets
```

CI additionally verifies the schema corpus, dependency policy, minimal `gix` feature graph, generated release assets,
Linux, macOS, Windows, and Rust 1.85.

## Boundaries

- **Always:** preserve read-only behavior, typed evidence, deterministic ordering, bounded work/output, explicit
  uncertainty, shared Markdown/JSON analysis, and black-box CLI verification.
- **Ask first:** add a non-grammar dependency, change schema semantics, broaden language promises beyond this V1
  list, alter cache retention/location, relax safety limits, or add framework-specific inference.
- **Never:** execute repository-controlled programs, follow paths outside scope, contact remotes, scrape editor or
  chat context, write inside target repositories, describe lexical evidence as semantic resolution, or hide
  omitted/partial analysis to improve apparent quality.

## V1 Release Gates

- The reading-plan and concise-history contract passes automated fixtures and manual usefulness review.
- Go, Lua, and Zig meet the same first-class support bar as existing languages.
- Generated/vendor/minified classification prevents low-value source from consuming default recommendations and
  quality status.
- Compact quality and `--strict` distinguish normal bounded projection from actionable degradation.
- Scale benchmarks enforce documented latency and output ceilings.
- The full cross-platform/MSRV/dependency-policy CI matrix is green.
- The release-binary project sweep completes without process failures, malformed reports, unexpected stderr, or
  a newly discovered P0 usability/correctness finding.

## Deferred Milestones

- Revision comparison remains a post-V1 navigation feature after the default reading plan is stable.
- F#, Elixir, C, and C++ remain candidates after V1 corpus evidence justifies their maintenance cost.
- Semantic-provider integration, framework-specific recommendations, and distributed query packs remain future
  work; they must not weaken the offline, read-only, evidence-labelled product contract.

## Risks and Open Questions

- A 5–10-path target can tempt the selector to pad weak recommendations. The implementation must prefer an
  explicit shortfall over low-confidence filler.
- Project-root balancing can conflict with task focus. Explicit focus wins, while the report records roots that
  received no recommendation and why.
- Generated/minified detection can misclassify maintained source. Rules must be deterministic, reported, tested,
  and safely overridable; the bounded heuristic is deliberately conservative and its thresholds are documented in
  the README.
- Go, Lua, and Zig grammar/query shapes will evolve independently. Pin versions and rerun each conformance corpus
  before upgrades.
- Compact quality semantics affect automation. Preserve old field meanings where possible; otherwise version the
  schema rather than silently changing strict-policy interpretation.
- Real-project usefulness remains partly subjective. Release review should record concrete failure modes and
  recommendation coverage, not reduce quality to one opaque score.

## Reference Material

- [Research notes](notes/README.md)
- [Implementation tickets](TODO.md)
- [V1 JSON schema](schema/v1/codeplat.json)
- [Release-readiness sweep](.sandbox/reports/README.md)
