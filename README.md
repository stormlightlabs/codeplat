# Codeplat

Codeplat (“code” + “plat” - a plan or map of land) is a CLI to help you orient
yourself in a new codebase.

It produces an integrated briefing, or a focused report when you need only one
evidence family:

- `codeplat map` inventories the current worktree and extracts structural maps for
  Rust, JavaScript, JSX, TypeScript, TSX, Python, Ruby, Java, C#, Go, and Lua files.
- `codeplat history` summarizes five Git-history signals
  1. churn
  2. contributors
  3. bug-related clusters
  4. monthly activity
  5. "firefighting"[^ff] language

## Quick start

Install the published crate with Cargo:

```sh
cargo install --locked codeplat
```

To build an exact source checkout instead, use the committed lockfile:

```sh
cargo build --locked --release
```

Then run it from a Git worktree:

```sh
codeplat
codeplat --json
codeplat map
codeplat map --json
codeplat map src --exclude 'src/generated/**' --json
codeplat map --recursive --json
codeplat history
codeplat history contributors src --json
codeplat explain src/map.rs --json
codeplat explain Parser --focus Parser --json
codeplat capabilities --json
codeplat doctor . --json
```

`PATH` defaults to the current directory. `codeplat` discovers the enclosing
Git repository and keeps the selected scope inside that repository.

## Default briefing

`codeplat [PATH]` starts with a repository overview and an ordered reading plan,
then includes up to five concise, evidence-backed history observations and brief
evidence notes. JSON retains the complete map and history report.

The source map accepts the same focus, token-budget, exclusion, cache, and color controls described below:

```sh
codeplat --focus parser --focus-path src --map-tokens 500 .
codeplat --no-cache --json .
codeplat --profile evidence --json .
```

The report keeps history caveats, source-map limitations, query-pack provenance,
partial-file diagnostics, and omitted-path reasons beside the evidence they qualify.

This makes unsupported or partially parsed files actionable instead of silently dropping them.

The default `compact` profile returns selected snippets and bounded samples of
files, symbols, edges, findings, omissions, and history evidence.

JSON reports include each collection's observed total, returned count, truncation
state, and reason.

Use `--profile evidence` for a larger, still resource-limited evidence sample.
Generated, vendored, minified, and source-map paths remain excluded in both
profiles unless selected with an exact `--focus-path`.

`--map-tokens` bounds the compact map payload; tiny budgets may return only the
highest-ranked snippet or summaries.

## Commands

### `codeplat map [OPTIONS] [PATH]`

The map command supports Rust, JavaScript, JSX, TypeScript, TSX, Python, Ruby, Java, C#, Go, and Lua source files.

An exact focus path can also include a classified `bin/` entry within the normal safety limits. It reports:

- tracked, modified, and untracked worktree state
- the selected language variant and file extension (`javascript_jsx` and `typescript_tsx` are explicit)
- definitions and lexical references with symbol kind, visibility, syntactic
  evidence, enclosing scope, 1-based source locations, and compact declaration context
- Go package and receiver scopes, import aliases, exported visibility, and `_test.go` declarations
- Lua local and global functions, dot and colon methods, variables, assignments, table fields, calls, and literal
  `require` module paths
- language- and import-aware lexical file edges with a resolution reason,
  confidence tier, candidate-group identity, and deterministic centrality ranking
- optional explicit `--focus` and `--focus-path` boosts
- repository landmarks for README and agent/contributor instructions, manifests and lockfiles,
  project roots, build/task entry points, test roots, CI, ownership, licenses, submodules, and
  nested repositories
- monorepo project-root groups with bounded source recommendations
- a bounded ranked selection controlled by `--map-tokens` (default: 1,000)
- parse errors, query-pack failures, grouped ambiguous lexical references, and
  unsupported/partial evidence per affected file
- non-source landmarks, configuration, documentation, and assets as `non_source`
  inventory omissions rather than unsupported programming-language evidence
- analyzed and omitted counts, repository root, scope, query-pack provenance, and
  supplied exclusions.

Lua module evidence is lexical. Literal `require("module.path")` calls can support file edges, but dynamic
`require` arguments, metatable behavior, and runtime table mutation are reported as limitations rather than resolved.

Exclusions can be repeated:

```sh
codeplat map --exclude 'src/generated/**' --exclude 'tests/fixtures/**'
```

Map focus and cache controls are explicit:

```sh
codeplat map --focus parser --focus-path src --map-tokens 500
codeplat map --cache always
codeplat map --cache files --cache-file src/parser.rs
codeplat map --cache manual
codeplat map --no-cache
codeplat map --recursive --no-cache
codeplat cache path
codeplat cache status
codeplat cache prune
codeplat cache clear
```

Profiles are selected with `--profile compact|evidence`. Compact is the default.

Nested repositories and checked-out submodules are boundaries by default. Use `--recursive` when
their source should be included; the boundary landmark remains in the report either way.

Compact analysis publishes these ceilings:

- 4,096 files
- 1 MiB per file
- 64 MiB of source bytes
- 2,048 syntax levels
- 20,000 symbols
- 32 lexical candidates per reference,
- 2,000 edges/findings
- 100,000 reachable commits
- 128 history evidence items per collection
- 30 seconds of analysis work,
- 8 MiB rendered report.

Landmark output is capped at 64 compact landmarks and 32 compact project roots, with totals and
truncation metadata preserved in JSON. Evidence mode raises those caps to the published report
limits.

Cache records are stored under `$XDG_CACHE_HOME/codeplat` (or `~/.cache/codeplat`) and
are reusable across map scopes.

### `codeplat explain <PATH-OR-SYMBOL> [PATH]`

Explain decomposes a bounded recommendation into typed focus, graph, ranking,
history-overlap, landmark, ambiguity, and omitted-alternative evidence.

### `codeplat history [OPERATION] [OPTIONS] [PATH]`

History analysis uses committed Git data only. The available operations are:

```text
history                 all five signals
history churn           changed-path frequency
history contributors    author concentration
history bugs            fix-related path clusters and churn overlap
history activity        author-date commits grouped by month
history firefighting    revert, hotfix, emergency, and rollback language
```

The default history window is 365 days; recent contributor concentration uses 180 days.
Override the windows or keyword sets explicitly, for example:

```sh
codeplat history bugs --window-days 30 --bug-keyword parser --json
codeplat history bugs --keyword-match substring --json
codeplat history contributors --include-emails --json
```

History output presents evidence and caveats. It does not treat churn, commit counts,
or commit-message matches as objective quality scores.

Bug and firefighting keywords use case-insensitive word-aware matching by default, and each
evidence commit records the terms it matched.

`--keyword-match substring` enables the former substring behavior explicitly.

Contributor concentration applies the `.mailmap` stored at the analyzed HEAD and records
raw-to-canonical identity mappings.

Compact output omits email addresses unless `--include-emails` is supplied.

Missing names are grouped as `Unknown`, and email matching is case-insensitive.

Churn keeps absolute commit counts and adds a rate per KiB using each path's current HEAD blob
size.

Empty, binary, generated, deleted, oversized, and resource-limited paths are labelled
explicitly.

Generated text is retained in normalization; empty, binary, deleted, oversized, and
resource-limited paths have no normalized rate.

Rename continuity is currently reported as unavailable, so exact-path counts never imply that earlier
history under another name was searched.

### `codeplat capabilities --json` and `codeplat doctor [PATH]`

`capabilities` reports the schema version, supported language grammars and query packs,
query-pack validity, and active compact/evidence.

`doctor` checks repository discovery, path-safety support, cache location and permissions,
the embedded schema, query packs, and effective limits.

## Output

Markdown is the default format. Use either `--format json` or `--json` for machine-readable output:

```sh
codeplat map --format json
codeplat history --json
```

JSON reports use `schema_version: 1`. Markdown and JSON are rendered from the same typed report model.
Reports go to stdout without ANSI escape sequences and diagnostics go to stderr.

Machine reports include typed provenance:

- the effective request and limits
- stable repository identity
- resolved HEAD reference/OID
- worktree state
- language/query-pack versions,
- cache state
- a UTC capture-date marker

History provenance records its observed committer-date range, author-versus-committer time
basis, current-HEAD semantics, and completeness status (`complete`, `shallow`, `missing_objects`, or `partial`).

The v1 JSON schema is [`schema/v1/codeplat.json`](schema/v1/codeplat.json), with compatibility
examples in [`schema/v1/golden`](schema/v1/golden).

Diagnostic color can be controlled with `--color auto|always|never` or `--no-color`.

Color settings never change report stdout.

[^ff]: https://newsroom.cisco.com/c/r/newsroom/en/us/a/y2024/m05/developers-spending-more-time-firefighting-issues-than-delivering-innovation.html
