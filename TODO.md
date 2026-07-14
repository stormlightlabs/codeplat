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

**Audit status:** Core commands exist, but path-scoped activity and envelope counts are reopened by Ticket 13.

**Acceptance criteria:**

- [x] Churn hotspots, contributor concentration, bug clusters with churn overlap, monthly activity,
      and firefighting commits are available in Markdown and JSON.
- [x] The default time windows and keyword patterns are visible in output and can be overridden explicitly.
- [ ] Analysis scopes every operation and envelope count to the requested directory within the discovered
      repository and never invokes the system `git` executable. Reopened by Ticket 13.
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

**Audit status:** Core inventory exists, but hidden-file semantics and hostile path containment are reopened
by Tickets 9 and 11.

**Acceptance criteria:**

- [x] The inventory includes every tracked eligible Rust file in scope, including tracked files that match an
      ignore pattern.
- [ ] Untracked, non-ignored Rust files, including hidden files, are included and labelled; ignored untracked
      files are omitted with a recorded reason. Reopened by Ticket 11.
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

- [x] JavaScript and TypeScript parsers and versioned query packs are registered through the same
      language-support interface as Rust.
- [x] Both languages extract tested definition, reference, scope, location, and snippet data for
      representative module and class/function structures.
- [x] JavaScript, TypeScript, and TSX/JSX extension handling is explicit and unambiguous in output.
- [x] Query failure or partial parsing is reported per file without losing findings from other languages.
- [x] Mixed Rust/JavaScript/TypeScript fixture output remains deterministic in both formats.

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

- [x] Python and Ruby parsers and versioned query packs are registered through the established
      language-support interface.
- [x] Fixtures prove extraction for representative module/class/function definitions, references,
      scopes, locations, and snippets in each language.
- [x] Dynamically dispatched or otherwise unresolved references remain labelled lexical and never
      become asserted call relationships.
- [x] Partial parse and query limitations appear per affected file in Markdown and JSON.
- [x] Mixed-language results preserve deterministic ordering and language-support metadata.

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

**Audit status:** Parser/query coverage exists, but visibility-aware ranking and Java field extraction are
reopened by Ticket 14.

**Acceptance criteria:**

- [x] Java and C# parsers and versioned query packs are registered through the established
      language-support interface.
- [x] Fixtures prove extraction for representative package/namespace, class, method, type, and
      reference structures appropriate to each grammar.
- [ ] Symbol visibility and duplicate names affect ranking data only; they never cause undisclosed omissions.
      Reopened by Ticket 14 because visibility is not represented in the report model.
- [x] Per-file parse/query limitations and unsupported extensions are carried through the shared report model.
- [x] Mixed-language output remains deterministic and preserves source locations/snippets accurately.

**Verification:**

- [x] Run `setaryb map --json` against Java, C#, and mixed-language fixtures;
      assert definitions, references, and limitations.
- [x] Review Markdown output for location and declaration-context readability.
- [x] `cargo fmt --check`
- [x] `cargo test`
- [x] `cargo clippy --all-targets --all-features -- -D warnings`

## 7. Make repository maps relevant, bounded, and cache-aware

**What to build:** Turn the structural map into a task-sensitive compact map with deterministic lexical
graph ranking, explicit focus boosts, token-budget selection, and all approved cache modes.

**Blocked by:** Ticket 3

**Acceptance criteria:**

- [x] File-level lexical dependency edges are built from typed definitions and references,
      preserve ambiguous candidates, and are never described as a type-resolved call graph.
- [x] Deterministic PageRank-style ranking prioritizes central files/symbols, then applies
      only explicit `--focus` and `--focus-path` boosts.
- [x] Generic and private-looking names may be downweighted as transparent ranking heuristics,
      but remain available in output when selected or requested.
- [x] `--map-tokens` defaults to 1,000 and selects structural snippets with location-preserving
      elision instead of full source bodies.
- [ ] Cache data lives only under a safe XDG Setaryb cache path and is isolated by repository, scope,
      query-pack, tool/schema, and source-content fingerprint. Reopened by Tickets 9 and 10 because an
      XDG path can resolve inside the repository and retention is unbounded.
- [ ] `auto`, `always`, `files`, `manual`, and `--no-cache` work as specified; manual mode labels
      possible staleness and never refreshes silently. Reopened by Ticket 10.

**Verification:**

- [ ] Exercise focus text/path, duplicate symbols, generic/private names, and complete-report token limits
      against the Rust and mixed-language fixtures. Reopened by Tickets 11 and 14.
- [ ] Use a temporary XDG cache directory to prove cache hits, automatic invalidation, exact explicit-file
      refresh, manual stale labels, and no-cache behavior. Reopened by Ticket 10.
- [x] Confirm deterministic JSON ordering across repeated runs with unchanged inputs.
- `cargo fmt --check`
- `cargo test`
- `cargo clippy --all-targets --all-features -- -D warnings`

## 8. Integrate the default briefing

**What to build:** Replace the successful foundation placeholder with one integrated `setaryb [PATH]`
briefing that joins history and source-map findings through the shared report model. This ticket completes
the product flow.

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

## 9. Enforce the hostile-repository trust boundary

**Priority:** P0 release blocker

**What to build:** Make every repository-derived path and Git configuration untrusted input. Setaryb
must prove that analysis cannot escape the selected scope or execute repository-controlled programs,
even for a malformed same-owner repository.

**Blocked by:** Ticket 3

**Acceptance criteria:**

- [ ] Tree, index, status, and walked paths remain byte-native until validated. Only non-empty relative
      normal components may reach a filesystem join; absolute, parent, platform-separator, NUL, and
      lossy-collision cases become typed safety diagnostics.
- [ ] Worktree reads reject symlinks or reparse points in every component and use a race-resistant
      beneath/no-follow strategy where the platform supports one. The resolved read remains under both
      repository root and selected scope.
- [ ] Repository opening and status collection use an explicit restrictive gix policy. Hooks,
      clean/smudge/process filters, credential helpers, editors, pagers, shell commands, and network
      transports cannot execute.
- [ ] An XDG cache root that resolves within the analyzed repository is rejected; a symlink cannot
      redirect cache writes into the repository.
- [ ] Malformed-tree, intermediate-symlink, swap-race, non-UTF-8, Windows-separator, and external-filter
      sentinel fixtures prove that outside content is neither read, emitted, nor cached and no sentinel runs.

**Verification:**

- Run the compiled CLI against every hostile fixture and assert the typed safety result and unchanged sentinel.
- Run platform-specific path tests on Linux, macOS, and Windows; document any weaker fallback explicitly.
- `cargo fmt --check`
- `cargo test --all-features`
- `cargo clippy --all-targets --all-features -- -D warnings`

## 10. Make cache modes correct, private, and maintainable

**Priority:** P0 release blocker

**What to build:** Replace the current fallback lookup with an explicit cache-mode state machine and
give users safe control over retained source-derived metadata.

**Blocked by:** Ticket 7

**Acceptance criteria:**

- [ ] In `auto`, a changed content fingerprint reparses current bytes and reports `refreshed`; it never
      returns stale tags. In `manual`, only the newest available record may be reused and is visibly stale.
- [ ] In `files`, only exactly normalized caller-named paths refresh. Matched, unmatched, unavailable,
      hit, miss, stale, and refreshed counts are distinct; cache-unavailable files are not counted as analyzed.
- [ ] Records are keyed by collision-resistant content identity plus exact grammar/query-pack content,
      are independent of report scope, and are written with user-private permissions using atomic replace.
- [ ] Concurrent readers/writers cannot observe truncated JSON or lose the newest record. Per-repository
      count/age/size retention is bounded and deterministic.
- [ ] `setaryb cache path|status|prune|clear` reports and controls retention without touching the target
      repository; `--no-cache` still performs no cache I/O.
- [ ] Regression tests prime a cache, edit source, exercise every mode, use duplicate basenames, pass an
      unmatched `--cache-file`, corrupt an entry, and run concurrent processes.

**Verification:**

- Run the cache fixture matrix under a temporary XDG directory and assert current symbols plus exact state counts.
- Inspect created directories/files for expected permissions and prove pruning never crosses the Setaryb cache root.
- `cargo fmt --check`
- `cargo test --all-features`
- `cargo clippy --all-targets --all-features -- -D warnings`

## 11. Bound analysis work and the complete emitted report

**Priority:** P0 release blocker

**What to build:** Make the default report genuinely compact and make hostile or very large repositories
produce deterministic partial evidence instead of unbounded traversal, allocation, recursion, or output.

**Blocked by:** Tickets 2 and 7

**Acceptance criteria:**

- [ ] The default compact profile emits selected snippets and bounded summaries only. Exhaustive symbols,
      edges, findings, omissions, and commit evidence require an explicit evidence profile or pagination.
- [ ] `--map-tokens` counts every data-dependent compact-map field and never reports an estimate above the
      requested budget. The fixed envelope is documented and bounded; property tests cover tiny budgets.
- [ ] Every collection exposes `total`, `returned`, and `truncated` plus a reason. Repeated ambiguity is grouped
      and sampled rather than emitted once per reference occurrence.
- [ ] Inventory prunes `.git`, ignored build/dependency/vendor directories, and nested repositories before
      descent. Hidden, non-ignored source remains eligible, and ignored omissions use bounded counts/samples.
- [ ] Reviewed limits cover file count/bytes, total bytes, syntax depth, symbols, candidate fan-out, edges,
      findings, commits, history evidence, elapsed work, and output. Oversize/binary/deep inputs become typed
      omissions; syntax traversal cannot overflow the call stack.
- [ ] Focused history uses an operation-aware streaming pass and computes tree diffs only when the requested
      evidence and path scope require them.
- [ ] Long scans expose concise TTY-only stderr progress, stay quiet when non-interactive, honor interruption
      promptly, and cannot leave a partial cache entry or JSON document described as successful.
- [ ] Benchmarks on Setaryb itself, a large ignored tree, high-ambiguity sources, and 10k/100k-commit fixtures
      enforce documented latency and output ceilings. The eight-file self-map is orders of magnitude smaller
      than the 7.33 MB audit baseline.

**Verification:**

- Assert byte size, estimated tokens, returned counts, and truncation metadata for compact and evidence profiles.
- Run resource fixtures under CI-friendly time and memory ceilings and confirm useful partial output.
- Test early pipe closure; a broken pipe terminates quietly rather than becoming exit 70.
- `cargo fmt --check`
- `cargo test --all-features`
- `cargo clippy --all-targets --all-features -- -D warnings`

## 12. Make machine reports reproducible and enforceable

**Priority:** P0 release blocker

**What to build:** Give agents enough typed provenance to decide whether a report is comparable and usable,
and turn schema version 1 into a real compatibility contract.

**Blocked by:** Tickets 1, 2, and 3

**Acceptance criteria:**

- [ ] Reports include tool version, effective command/options and limits, repository identity, resolved HEAD
      ref/OID, capture/reference time, worktree snapshot state, grammar/query-pack versions, and cache state.
- [ ] History reports include observed date range, author-versus-committer time basis, current-HEAD semantics,
      and shallow/partial/missing-object completeness. Incomplete history is typed, not authoritative-looking.
- [ ] Byte-native Git paths remain distinct internally and have one reversible JSON representation plus a
      readable escaped Markdown representation; invalid UTF-8 and case-collision fixtures do not merge paths.
- [ ] A committed JSON Schema and golden v1 corpus validate every report variant. CI rejects incompatible
      removal, retyping, or semantic reuse without a schema-version change.
- [ ] A documented strict policy lets callers fail on stale, truncated, incomplete, unsupported, or partial
      results without parsing prose; default mode still returns useful typed partial evidence.
- [ ] `setaryb capabilities --json` reports schema/query-pack/language support and active limit defaults without
      running repository analysis.
- [ ] `setaryb doctor [PATH]` checks discovery, path-safety support, cache location/permissions, schema/query
      availability, and effective limits without emitting source-derived evidence or changing repository state.

**Verification:**

- Validate all JSON fixtures against the schema and deserialize historical v1 golden documents.
- Run the CLI against shallow, missing-object, dirty-worktree, non-UTF-8, and strict-policy fixtures.
- `cargo fmt --check`
- `cargo test --all-features`
- `cargo clippy --all-targets --all-features -- -D warnings`

## 13. Correct and strengthen Git-history evidence

**Priority:** P0 correctness for scope/provenance; P1 for signal enrichment

**What to build:** Make every history result obey its advertised scope and improve the article-derived
signals without hiding their original evidence or caveats.

**Blocked by:** Tickets 2 and 12

**Acceptance criteria:**

- [ ] `history activity PATH` and `commits_seen` count only commits affecting the selected scope. Any
      intentionally repository-wide section carries a separate explicit scope instead of inheriting `PATH`.
- [ ] Contributor concentration applies `.mailmap` by default, records raw-to-canonical provenance, handles
      missing/case-varied identity, and omits email from compact output unless explicitly requested.
- [ ] Bug/firefighting evidence records the matched term and uses word-aware matching by default, with an
      explicit substring mode for compatibility; `fixture`, `prefix`, and `debug` are negative tests.
- [ ] Size-normalized churn is reported beside—not instead of—absolute churn, with the size basis and zero/
      generated/binary-file behavior explicit.
- [ ] Rename-aware continuity is either implemented with evidence and limits or reported as unavailable;
      a renamed hotspot never silently appears to have no earlier history.
- [ ] Focused operations decode and diff only required data, preserve deterministic ties, and expose bounded
      totals/evidence according to Ticket 11.

**Verification:**

- Run scoped activity, alias/mailmap, keyword-boundary, rename, and normalized-churn fixtures in both formats.
- Compare compact results with manually established fixture truth, including negative keyword cases.
- `cargo fmt --check`
- `cargo test --all-features`
- `cargo clippy --all-targets --all-features -- -D warnings`

## 14. Make lexical maps high-signal and explainable

**Priority:** P0 quality gate for the v1 map

**What to build:** Replace global bare-name fan-out with bounded, confidence-aware lexical evidence and
measure each first-class language against a conformance corpus.

**Blocked by:** Tickets 4, 5, 6, 7, and 11

**Acceptance criteria:**

- [ ] Candidate matching is constrained by language plus import/module/scope information where available.
      Same-file and cross-language bare-name matches do not affect centrality without explicit evidence.
- [ ] Each retained edge carries a resolution reason and confidence tier. Candidate groups are interned,
      deduplicated by path/symbol identity, fan-out capped, and repeated ambiguity findings grouped.
- [ ] Centrality excludes or sharply discounts locals, fields, generic identifiers, imports masquerading as
      definitions, and unresolved property names; raw tags remain available in the explicit evidence profile.
- [ ] `setaryb explain <PATH-OR-SYMBOL>` decomposes focus, history, landmark, graph, ranking, ambiguity, and
      omission contributions without describing lexical evidence as semantic truth.
- [ ] Every first-class language has positive/negative conformance fixtures for definitions, references,
      imports/aliases, visibility, nesting, overloads/generics as applicable, malformed input, and conventional
      extensionless entry files. The declared Java field capability and empty-map provenance are accurate.
- [ ] Parsers and compiled queries are reused per language, incoming counts are preaggregated, and benchmarks
      prevent recurrence of candidate-copy and edge-rescan amplification.

**Verification:**

- Assert graph candidates, confidence reasons, grouped ambiguity, and ranking against hand-reviewed mixed-language fixtures.
- Run the self-map and verify core analyzer files outrank repeated field names under the default profile.
- Record output size, candidate fan-out, and latency before and after the change.
- `cargo fmt --check`
- `cargo test --all-features`
- `cargo clippy --all-targets --all-features -- -D warnings`

## 15. Add repository landmarks and project topology

**Priority:** P1 post-v1 navigation feature

**What to build:** Help users choose the first non-source files and project roots to read before symbol-level
navigation, using bounded presence-based evidence rather than speculative framework semantics.

**Blocked by:** Tickets 8, 11, and 12

**Acceptance criteria:**

- [ ] The integrated briefing identifies README/contributor/agent instructions, manifests and lockfiles,
      workspace/package roots, build/task entry points, test roots, CI, ownership files, licenses, submodules,
      and nested repositories within scope.
- [ ] Every landmark includes a stable kind, path, detection reason, project-root association, and worktree
      state. Unknown or conflicting files remain `unknown`; contents are read only within shared size/safety limits.
- [ ] Monorepo output groups landmarks and source recommendations by detected project root without inventing
      dependencies between packages.
- [ ] Compact output returns a bounded, prioritized landmark set plus totals; the evidence profile can expose
      the complete inventory through pagination.
- [ ] Focus paths and explicit exclusions apply consistently, and submodules/nested repositories are boundaries
      unless the caller explicitly requests recursive analysis.

**Verification:**

- Exercise Rust, Node/TypeScript, Python, Ruby, Java, .NET, and mixed-monorepo landmark fixtures.
- Assert AGENTS/README/build/test/CI/ownership priority and safe nested-repository/submodule behavior.
- `cargo fmt --check`
- `cargo test --all-features`
- `cargo clippy --all-targets --all-features -- -D warnings`

## 16. Compare repository orientation across revisions

**Priority:** P1 post-v1 navigation feature

**What to build:** Let a user or agent compare two explicit repository states and see bounded changes to the
evidence that should alter what they read next.

**Blocked by:** Tickets 12, 14, 15, and 17

**Acceptance criteria:**

- [ ] `setaryb compare --base <REV> [--head <REV|worktree>] [PATH]` resolves and records both states without
      checkout, hooks, filters, network, or target-repository mutation.
- [ ] Markdown and JSON report added/removed/changed landmarks, public definitions, qualified lexical evidence,
      history hotspots, ownership concentration, and recommended next reads with before/after provenance.
- [ ] Rename and ambiguity handling is explicit. A ranking change includes its evidence delta rather than only
      two opaque scores.
- [ ] All collections obey compact/evidence profiles and limits; partial availability on either side is visible
      and compatible with strict policy.
- [ ] Comparison extends the normative schema and compatibility corpus without accepting Markdown as input.

**Verification:**

- Compare fixture revisions covering rename, public API change, manifest/test/CI change, and a dirty worktree.
- Assert no checkout/worktree mutation and deterministic output for fixed object IDs.
- `cargo fmt --check`
- `cargo test --all-features`
- `cargo clippy --all-targets --all-features -- -D warnings`

## 17. Ship a trustworthy distributable v1

**Priority:** P0 final release gate

**What to build:** Turn the integrated CLI into a supportable package with a minimal
dependency/trust surface and reproducible cross-platform verification.

**Blocked by:** Tickets 8 through 14

**Acceptance criteria:**

- [ ] `gix` disables default features and enables only the reviewed discovery/object/revision/index/status
      capabilities Setaryb needs. `cargo tree -e features` is recorded, and executable/network/credential/
      mutation surface is absent or unreachable with a regression proof.
- [ ] Cargo metadata includes description, license, repository, README, Rust version, keywords, and categories;
      the repository contains the selected license and documented install/uninstall/cache-cleanup instructions.
- [ ] CI covers formatting, all-feature tests, Clippy with warnings denied, docs, package verification, JSON Schema
      compatibility, dependency license/advisory policy, Linux, macOS, Windows, and the declared MSRV.
- [ ] Dependency advisory, license/source, duplicate-version, and unused-capability policy is documented and
      automated. Release artifacts have checksums and reproducible build instructions.
- [ ] Help, README, roadmap, and support links match implemented Java/C# and deferred-language status; shell
      completions and man pages are generated from Clap; broken pipes are quiet and exit behavior follows CLI conventions.
- [ ] The current large map/report modules are separated along existing inventory, query, graph, budget, cache,
      schema, and rendering responsibilities without changing the black-box contract or adding abstraction for its own sake.
- [ ] The default Markdown and JSON briefings pass a manual usefulness review on a small project, Setaryb,
      and a mixed-language monorepo. No known P0 finding is waived without a recorded rationale and expiry.

**Verification:**

- `cargo fmt --check`
- `cargo test --workspace --all-features`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo doc --workspace --all-features --no-deps`
- `cargo package`
- Inspect `cargo tree -e features -i gix`, packaged contents, release artifact checksums, completions, and man pages.

## Frontier

Tickets 1 through 7 are implemented, but audit evidence reopens parts of Tickets 2, 3, and 7 through
new acceptance tests. Ticket 8 can integrate the default report while Tickets 9 through 14 proceed in
parallel where their code boundaries permit. Ticket 17 is the v1 release gate and waits for Tickets 8
through 14. Tickets 15 and 16 are post-v1 improvements; landmarks precede comparison because comparison
needs a stable topology model.

Recommended order within the stabilization queue:

1. Ticket 9 first: stop reading outside the repository or executing repository-controlled programs.
2. Tickets 10 and 11 next: stale and unbounded output invalidate everything downstream.
3. Tickets 12 and 13: make machine/history evidence reproducible and correctly scoped.
4. Ticket 14: improve map signal only after its inputs and resource envelope are trustworthy.
5. Ticket 8 integration, then Ticket 17 release hardening.
