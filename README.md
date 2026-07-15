# Setaryb

Setaryb (Setāreyāb, Persian for “astrolabe”) is a CLI to help you orient yourself
in a new codebase.

It produces two focused reports:

- `map` inventories the current worktree and extracts structural maps for Rust, JavaScript,
  JSX, TypeScript, TSX, Python, Ruby, Java, and C# source files.
- `history` summarizes five Git-history signals: churn, contributors, bug-related clusters,
  monthly activity, and firefighting language.

## Quick start

Build the binary with Cargo:

```sh
cargo build --release
```

Then run it from a Git worktree:

```sh
./target/release/setaryb map
./target/release/setaryb map --json
./target/release/setaryb map src --exclude 'src/generated/**' --json
./target/release/setaryb history
./target/release/setaryb history contributors src --json
```

`PATH` defaults to the current directory. `setaryb` discovers the enclosing
Git repository and keeps the selected scope inside that repository.

## Commands

### `setaryb map [OPTIONS] [PATH]`

The map command supports Rust, JavaScript, JSX, TypeScript, TSX, Python, Ruby, Java, and C# source files. It reports:

- tracked, modified, and untracked worktree state
- the selected language variant and file extension (`javascript_jsx` and `typescript_tsx` are explicit)
- definitions and lexical references with symbol kind, enclosing scope,
  1-based source locations, and compact declaration context
- lexical file edges and deterministic centrality ranking, with optional explicit
  `--focus` and `--focus-path` boosts
- a bounded ranked selection controlled by `--map-tokens` (default: 1,000)
- parse errors, query-pack failures, and ambiguous lexical references per affected file
- analyzed and omitted counts, repository root, scope, query-pack provenance, and
  supplied exclusions.

Exclusions can be repeated:

```sh
setaryb map --exclude 'src/generated/**' --exclude 'tests/fixtures/**'
```

Map focus and cache controls are explicit:

```sh
setaryb map --focus parser --focus-path src --map-tokens 500
setaryb map --cache always
setaryb map --cache files --cache-file src/parser.rs
setaryb map --cache manual
setaryb map --no-cache
```

Cache records are stored under `$XDG_CACHE_HOME/setaryb` (or `~/.cache/setaryb`) and
are keyed by repository, scope, query pack, tool schema, path, and source content.

Manual mode never refreshes silently and labels stale or unavailable records.

### `setaryb history [OPERATION] [OPTIONS] [PATH]`

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
setaryb history bugs --window-days 30 --bug-keyword parser --json
```

History output presents evidence and caveats. It does not treat churn, commit counts,
or commit-message matches as objective quality scores.

## Output

Markdown is the default format. Use either `--format json` or `--json` for machine-readable output:

```sh
setaryb map --format json
setaryb history --json
```

JSON reports use `schema_version: 1`. Markdown and JSON are rendered from the same typed report model.
Reports go to stdout without ANSI escape sequences and diagnostics go to stderr.

Diagnostic color can be controlled with `--color auto|always|never` or `--no-color`.
Color settings never change report stdout.

## Map limitations

The source map is lexical. It does not resolve imports, types, macros, runtime
behavior, or semantic call relationships. A name with multiple definition candidates
is reported as ambiguous instead of being presented as a resolved edge.

Parse and query limitations remain attached to the affected file so other language
findings are retained.

The ranked map uses lexical PageRank-style centrality.

Generic and underscore-prefixed names may be downweighted for ranking only but remain
available in the full JSON evidence. The token budget bounds selected structural snippets
and keeps their source locations when context is elided.

Unsupported files, read failures, symlinks, and partial parses remain visible in the report.

Java and C# query-pack provenance is reported as `java-v1` and `c-sharp-v1` in mixed-language
maps. Their lexical findings include package/namespace, type, method, property, and field
symbols where the grammar exposes them; visibility is not treated as a reason to omit a symbol.
