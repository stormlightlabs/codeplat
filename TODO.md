# Tickets: Setaryb v1 repository navigation CLI

Implementation tickets derived from [ROADMAP.md](ROADMAP.md).

## 1. Establish the CLI contract and verification foundation

**What to build:** Create the typed command and report foundation that every Setaryb feature uses:
the Clap command hierarchy, versioned report model, Markdown and JSON renderers, stdout/stderr
policy, and black-box fixture-test harness.

**Blocked by:** None - can start immediately

**Acceptance criteria:**

- [x] `setaryb`, `setaryb map`, and `setaryb history` parse with concise no-argument
      guidance and complete `--help` text.
- [x] `--format markdown`, `--format json`, and `--json` select a shared typed report
      renderer; JSON includes `schema_version: 1` and Markdown is generated directly
      without a Markdown parser.
- [x] Report stdout has no ANSI/control sequences, diagnostics use stderr, and parser/usage
      errors have stable documented exit categories.
- [x] Color policy supports `auto`, `always`, and `never`, honors `NO_COLOR`, and does not
      affect report stdout.
- [x] A black-box CLI test harness can execute fixture repositories with a temporary
      XDG cache location and assert parsed JSON plus Markdown snapshots.
- [x] The selected baseline dependencies use compatible, locked versions and no dependency
      beyond the approved roadmap stack is introduced.

**Verification:**

- `cargo fmt --check`
- `cargo test`
- `cargo clippy --all-targets --all-features -- -D warnings`
- Run `setaryb --help`, `setaryb --json`, and `setaryb --color never --json` against a minimal fixture;
  confirm stdout is parseable JSON without ANSI escapes.

## 2. Deliver the complete Git-history briefing

**What to build:** Let a user or agent inspect the five history signals through `setaryb history`
and focused history commands, using gix only and returning structured evidence with limitations.

**Blocked by:** Ticket 1

**Acceptance criteria:**

- [x] Churn hotspots, contributor concentration, bug clusters with churn overlap, monthly activity,
      and firefighting commits are available in Markdown and JSON.
- [x] The default time windows and keyword patterns are visible in output and can be overridden explicitly.
- [x] Analysis scopes paths to the requested directory within the discovered repository and never
      invokes the system `git` executable.
- [x] Results identify the evidence paths/commits and attach the required caveats for absolute churn,
      squash merges, weak commit messages, activity interpretation, and empty firefighting output.
- [x] The report does not label people or paths as objectively bad; it presents priorities and uncertainty.
- [x] Fixture repositories cover non-merge filtering, overlapping churn/fix paths, keyword misses, and monthly grouping.

**Verification:**

- Run every `setaryb history` operation against the history fixture in Markdown and JSON.
- Assert the JSON findings semantically, including caveats and overlap paths.
- `cargo fmt --check`
- `cargo test`
- `cargo clippy --all-targets --all-features -- -D warnings`

## 3. Deliver worktree inventory and a Rust source map

**What to build:** Let callers run `setaryb map` on a current worktree and obtain a Rust structural map with
symbol locations, declaration snippets, references, worktree-state labels, and honest language/parse limitations.

**Blocked by:** Ticket 1

**Acceptance criteria:**

- [x] The inventory includes every tracked eligible Rust file in scope, including tracked files that match an
      ignore pattern.
- [x] Untracked, non-ignored Rust files are included and labelled; ignored untracked files are omitted with a
      recorded omission reason.
- [x] `ignore` is used for traversal and explicit ignore/glob matching, while `gix` provides repository and
      tracked/worktree-state data.
- [x] Rust Tree-sitter queries extract definitions and references with symbol kind, scope, source location, and
      compact declaration/signature context.
- [x] Parse errors, unsupported files, and ambiguous lexical relationships are explicit findings rather than
      silent omissions or false semantic claims.
- [x] Scope, supplied exclusions, analyzed/omitted counts, and tracked/modified/untracked state are available in
      both formats.

**Verification:**

- Run `setaryb map` against fixtures containing tracked, modified, untracked, ignored, and malformed Rust files.
- Confirm JSON locations and worktree labels; review the Markdown map for readable snippets and limitation text.
- `cargo fmt --check`
- `cargo test`
- `cargo clippy --all-targets --all-features -- -D warnings`

## 4. Add JavaScript and TypeScript map support

**What to build:** Extend the source-map registry so JavaScript and TypeScript receive the same first-class
structural-map contract as Rust.

**Blocked by:** Ticket 3

**Acceptance criteria:**

- [ ] JavaScript and TypeScript parsers and versioned query packs are registered through the same
      language-support interface as Rust.
- [ ] Both languages extract tested definition, reference, scope, location, and snippet data for
      representative module and class/function structures.
- [ ] JavaScript, TypeScript, and TSX/JSX extension handling is explicit and unambiguous in output.
- [ ] Query failure or partial parsing is reported per file without losing findings from other languages.
- [ ] Mixed Rust/JavaScript/TypeScript fixture output remains deterministic in both formats.

**Verification:**

- Run `setaryb map --json` against JavaScript, TypeScript, and mixed-language fixtures;
  assert language status and selected tags.
- Review the Markdown output for declaration context and partial-support notices.
- `cargo fmt --check`
- `cargo test`
- `cargo clippy --all-targets --all-features -- -D warnings`

## 5. Add Python and Ruby map support

**What to build:** Extend the source-map registry so Python and Ruby receive the same
first-class structural-map contract as Rust.

**Blocked by:** Ticket 3

**Acceptance criteria:**

- [ ] Python and Ruby parsers and versioned query packs are registered through the established
      language-support interface.
- [ ] Fixtures prove extraction for representative module/class/function definitions, references,
      scopes, locations, and snippets in each language.
- [ ] Dynamically dispatched or otherwise unresolved references remain labelled lexical and never
      become asserted call relationships.
- [ ] Partial parse and query limitations appear per affected file in Markdown and JSON.
- [ ] Mixed-language results preserve deterministic ordering and language-support metadata.

**Verification:**

- Run `setaryb map` in both output formats against Python, Ruby, and mixed-language fixtures.
- Assert parsed JSON findings, support statuses, and caveats.
- `cargo fmt --check`
- `cargo test`
- `cargo clippy --all-targets --all-features -- -D warnings`

## 6. Add Java and C# map support

**What to build:** Extend the source-map registry so Java and C# receive the same first-class
structural-map contract as Rust.

**Blocked by:** Ticket 3

**Acceptance criteria:**

- [ ] Java and C# parsers and versioned query packs are registered through the established
      language-support interface.
- [ ] Fixtures prove extraction for representative package/namespace, class, method, type, and
      reference structures appropriate to each grammar.
- [ ] Symbol visibility and duplicate names affect ranking data only; they never cause undisclosed omissions.
- [ ] Per-file parse/query limitations and unsupported extensions are carried through the shared report model.
- [ ] Mixed-language output remains deterministic and preserves source locations/snippets accurately.

**Verification:**

- Run `setaryb map --json` against Java, C#, and mixed-language fixtures;
  assert definitions, references, and limitations.
- Review Markdown output for location and declaration-context readability.
- `cargo fmt --check`
- `cargo test`
- `cargo clippy --all-targets --all-features -- -D warnings`

## 7. Make repository maps relevant, bounded, and cache-aware

**What to build:** Turn the structural map into a task-sensitive compact map with deterministic lexical
graph ranking, explicit focus boosts, token-budget selection, and all approved cache modes.

**Blocked by:** Ticket 3

**Acceptance criteria:**

- [ ] File-level lexical dependency edges are built from typed definitions and references,
      preserve ambiguous candidates, and are never described as a type-resolved call graph.
- [ ] Deterministic PageRank-style ranking prioritizes central files/symbols, then applies
      only explicit `--focus` and `--focus-path` boosts.
- [ ] Generic and private-looking names may be downweighted as transparent ranking heuristics,
      but remain available in output when selected or requested.
- [ ] `--map-tokens` defaults to 1,000 and selects structural snippets with location-preserving
      elision instead of full source bodies.
- [ ] Cache data lives only under the XDG Setaryb cache path and is isolated by repository, scope,
      query-pack, tool/schema, and source-content fingerprint.
- [ ] `auto`, `always`, `files`, `manual`, and `--no-cache` work as specified; manual mode labels
      possible staleness and never refreshes silently.

**Verification:**

- Exercise focus text/path, duplicate symbols, generic/private names, and token limits against
  the Rust and mixed-language fixtures.
- Use a temporary XDG cache directory to prove cache hits, invalidation, explicit file refresh,
  manual stale labels, and no-cache behavior.
- Confirm deterministic JSON ordering across repeated runs with unchanged inputs.
- `cargo fmt --check`
- `cargo test`
- `cargo clippy --all-targets --all-features -- -D warnings`

## 8. Release the integrated v1 briefing

**What to build:** Make `setaryb [PATH]` a release-quality integrated briefing that joins complete
history and source-map findings, finishes CLI usability and documentation, and proves the public
contract across the full first-class language set.

**Blocked by:** Tickets 2, 4, 5, 6, and 7

**Acceptance criteria:**

- [ ] The default command combines the five history diagnostics and the ranked source map in one
      shared Markdown/JSON report without duplicating or re-parsing data.
- [ ] All first-class languages have query-pack provenance, support-status documentation, fixture
      coverage, and actionable unsupported/partial-analysis messages.
- [ ] Top-level and subcommand help lead with examples, document output/cache/color/scope/focus semantics,
      and give users an issue or support path.
- [ ] JSON schema compatibility tests, Markdown readability snapshots, exit-code checks, and no-ANSI-stdout
      checks cover the final command surface.
- [ ] Read-only, no-network, no-hook/filter, no-editor/chat-context, no-project-cache, and symlink-scope
      safeguards are covered by regression tests and documentation.
- [ ] README and roadmap accurately distinguish v1 support from the planned F#, Go, Elixir, C, and
      C++ expansion.

**Verification:**

- Run the full compiled CLI suite against every fixture repository in both formats and each cache/color mode.
- Manually inspect the default Markdown briefing for a supported mixed-language repository and a
  repository with partial/unsupported analysis.
- `cargo fmt --check`
- `cargo test`
- `cargo clippy --all-targets --all-features -- -D warnings`

## Frontier

Start with Ticket 1. Once it is complete, Tickets 2 and 3 can proceed in parallel.
After Ticket 3, Tickets 4, 5, 6, and 7 can proceed independently. Ticket 8 is the
final integration and release gate.
