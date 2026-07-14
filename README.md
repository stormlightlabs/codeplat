# Setaryb

Setaryb (Setāreyāb, Persian for “astrolabe”) is a CLI to help you orient yourself
in a new codebase.

It produces two focused reports:

- `map` inventories the current worktree and extracts structural maps for Rust, JavaScript,
  JSX, TypeScript, and TSX source files.
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

The map command supports Rust, JavaScript, JSX, TypeScript, and TSX source files. It reports:

- tracked, modified, and untracked worktree state
- the selected language variant and file extension (`javascript_jsx` and `typescript_tsx` are explicit)
- definitions and lexical references with symbol kind, enclosing scope,
  1-based source locations, and compact declaration context
- parse errors, query-pack failures, and ambiguous lexical references per affected file
- analyzed and omitted counts, repository root, scope, query-pack provenance, and
  supplied exclusions.

Exclusions can be repeated:

```sh
setaryb map --exclude 'src/generated/**' --exclude 'tests/fixtures/**'
```

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
is reported as ambiguous instead of being presented as a resolved edge. Parse and
query limitations remain attached to the affected file so other language findings
are retained.

Unsupported files, read failures, symlinks, and partial parses remain visible in
the report.

### Coming Soon

- Python, Ruby, Java, and C# support
- Ranking, token budgeting, caching, and the integrated default briefing
