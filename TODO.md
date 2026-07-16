# Tickets: Codeplat v1 repository navigation CLI

Implementation tickets derived from [ROADMAP.md](ROADMAP.md).

**Last reviewed:** 2026-07-15

## 1. Establish the CLI contract and verification foundation

Created the typed command and report foundation that every Codeplat feature uses: the Clap command hierarchy,
versioned report model, Markdown and JSON renderers, stdout/stderr policy, and black-box fixture-test harness.

## 2. Deliver the complete Git-history briefing

Lets a user or agent inspect the five history signals through `codeplat history`
and focused history commands, using gix only and returning structured evidence with limitations.

## 3. Deliver worktree inventory and a Rust source map

Lets callers run `codeplat map` on a current worktree and obtain a Rust structural map with
symbol locations, declaration snippets, references, worktree-state labels, and honest language/parse limitations.

## 4. Add JavaScript and TypeScript map support

Extended the source-map registry so JavaScript and TypeScript receive the same first-class
structural-map contract as Rust.

## 5. Add Python and Ruby map support

Extended the source-map registry so Python and Ruby receive the same
first-class structural-map contract as Rust.

## 6. Add Java and C# map support

**What to build:** Extend the source-map registry so Java and C# receive the same first-class
structural-map contract as Rust.

**Blocked by:** Ticket 3

**Audit status:** Parser/query coverage exists and Java field extraction is now covered by the query pack and
regression tests. Visibility-aware ranking remains reopened by Ticket 14 because visibility is not represented
in the report model.

**Acceptance criteria:**

- [x] Java and C# parsers and versioned query packs are registered through the established
      language-support interface.
- [x] Fixtures prove extraction for representative package/namespace, class, method, type, and
      field, and reference structures appropriate to each grammar.
- [ ] Symbol visibility and duplicate names affect ranking data only; they never cause undisclosed omissions.
      Reopened by Ticket 14 because visibility is not represented in the report model.
- [x] Per-file parse/query limitations and unsupported extensions are carried through the shared report model.
- [x] Mixed-language output remains deterministic and preserves source locations/snippets accurately.

**Verification:**

- [x] Run `codeplat map --json` against Java, C#, and mixed-language fixtures;
      assert definitions, references, and limitations.
- [x] Review Markdown output for location and declaration-context readability.
- [x] `cargo fmt --check`
- [x] `cargo test`
- [x] `cargo clippy --all-targets --all-features -- -D warnings`

## 7. Make repository maps relevant, bounded, and cache-aware

**What to build:** Turn the structural map into a task-sensitive compact map with deterministic lexical
graph ranking, explicit focus boosts, token-budget selection, and all approved cache modes.

**Blocked by:** Ticket 3

**Implementation status (2026-07-15):** Ranking, explicit focus boosts, bounded structural selection, and
deterministic JSON ordering are implemented and covered by tests. Ticket 10 completes cache identity, mode,
retention, privacy, atomic-write, and observability behavior. Generic/private ranking and complete-report
budget work remains with Tickets 11 and 14.

**Acceptance criteria:**

- [x] File-level lexical dependency edges are built from typed definitions and references,
      preserve ambiguous candidates, and are never described as a type-resolved call graph.
- [x] Deterministic PageRank-style ranking prioritizes central files/symbols, then applies
      only explicit `--focus` and `--focus-path` boosts.
- [x] Generic and private-looking names may be downweighted as transparent ranking heuristics,
      but remain available in output when selected or requested.
- [x] `--map-tokens` defaults to 1,000 and selects structural snippets with location-preserving
      elision instead of full source bodies.
- [x] Cache data lives only under a safe XDG Codeplat cache path and is isolated by canonical repository,
      exact path, query-pack content, tool/schema, and source-content fingerprint. Cache roots resolving
      inside the repository are rejected and retention is bounded by Ticket 10.
- [x] `auto`, `always`, `files`, `manual`, and `--no-cache` work as specified; manual mode labels
      possible staleness and never refreshes silently.

**Verification:**

- [ ] Exercise focus text/path, duplicate symbols, generic/private names, and complete-report token limits
      against the Rust and mixed-language fixtures. Focus/path, duplicate-symbol, and selection-budget coverage
      exists; generic/private ranking and complete-report limits remain reopened by Tickets 11 and 14.
- [x] Use a temporary XDG cache directory to prove cache hits, automatic invalidation, exact explicit-file
      refresh, manual stale labels, no-cache behavior, retention, unavailable/unmatched-file accounting,
      permissions, corruption recovery, and concurrency.
- [x] Confirm deterministic JSON ordering across repeated runs with unchanged inputs.
- `cargo fmt --check`
- `cargo test`
- `cargo clippy --all-targets --all-features -- -D warnings`

## 8. Integrate the default briefing

**What to build:** Replace the successful foundation placeholder with one integrated `codeplat [PATH]`
briefing that joins history and source-map findings through the shared report model. This ticket completes
the product flow.

**Implementation status (2026-07-15):** The default command now renders one analyzed report containing all
five history signals and the ranked source map in Markdown or JSON. Existing language provenance and
partial/unsupported analysis diagnostics remain attached to the map evidence. Trust-boundary and resource
hardening follow-up remains tracked by Tickets 9 through 14.

**Blocked by:** Tickets 2, 4, 5, 6, and 7

**Acceptance criteria:**

- [x] The default command combines the five history diagnostics and the ranked source map in one
      shared Markdown/JSON report without duplicating or re-parsing data.
- [x] All first-class languages have query-pack provenance, support-status documentation, fixture
      coverage, and actionable unsupported/partial-analysis messages.
- [x] Top-level and subcommand help lead with examples, document output/cache/color/scope/focus semantics,
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

**What to build:** Make every repository-derived path and Git configuration untrusted input. Codeplat
must prove that analysis cannot escape the selected scope or execute repository-controlled programs,
even for a malformed same-owner repository.

**Blocked by:** Ticket 3

**Implementation status (2026-07-15):** Repository discovery, tree/index/status/walk path validation,
scope containment, no-follow worktree reads, restrictive gix opening, and cache-root/write containment
are implemented in `src/security.rs`. Unix reads and cache writes use descriptor-relative no-follow
traversal; Windows and other non-Unix targets use component reparse checks plus the documented weaker
standard-library fallback. Hostile CLI fixtures cover malformed tree paths, worktree symlinks, cache-root
symlinks, non-UTF-8 names where the host filesystem permits them, and filter sentinels. Cross-platform
execution of the full fixture matrix remains a release verification task.

**Acceptance criteria:**

- [x] Tree, index, status, and walked paths remain byte-native until validated. Only non-empty relative
      normal components may reach a filesystem join; absolute, parent, platform-separator, NUL, and
      lossy-collision cases become typed safety diagnostics.
- [x] Worktree reads reject symlinks or reparse points in every component and use a race-resistant
      beneath/no-follow strategy where the platform supports one. The resolved read remains under both
      repository root and selected scope.
- [x] Repository opening and status collection use an explicit restrictive gix policy. Hooks,
      clean/smudge/process filters, credential helpers, editors, pagers, shell commands, and network
      transports cannot execute.
- [x] An XDG cache root that resolves within the analyzed repository is rejected; a symlink cannot
      redirect cache writes into the repository.
- [x] Malformed-tree, intermediate-symlink, swap-race, non-UTF-8, Windows-separator, and external-filter
      sentinel fixtures prove that outside content is neither read, emitted, nor cached and no sentinel runs.
      The compiled suite covers the malformed-tree, intermediate-symlink, swap-race, and filter cases;
      path-validation tests cover non-UTF-8 and Windows-separator inputs. Full Windows/macOS fixture
      execution remains platform CI work.

**Verification:**

- [x] Run the compiled CLI against the available hostile fixtures and assert the typed safety result and unchanged sentinel.
- [ ] Run platform-specific path tests on Linux, macOS, and Windows; document any weaker fallback explicitly.
      Unix uses descriptor-relative `openat`/`O_NOFOLLOW`; non-Unix targets use component reparse checks
      and standard-library reads/writes, which are weaker against a concurrent rename/symlink race.
- [x] `cargo fmt --check`
- [x] `cargo test --all-features`
- [x] `cargo clippy --all-targets --all-features -- -D warnings`

## 10. Make cache modes correct, private, and maintainable

Replace the current fallback lookup with an explicit cache-mode state machine and
give users safe control over retained source-derived metadata.

## 11. Bound analysis work and the complete emitted report

**Priority:** P0 release blocker

**What to build:** Make the default report genuinely compact and make hostile or very large repositories
produce deterministic partial evidence instead of unbounded traversal, allocation, recursion, or output.

**Blocked by:** Tickets 2 and 7

**Implementation status (2026-07-15):** Compact and evidence profiles now project
bounded collections with deterministic totals, returned counts, truncation reasons,
published resource ceilings, bounded source/cache reads, iterative tree and syntax
traversal, operation-aware history collection, and quiet broken-pipe handling. The
default self-map is approximately 13 KB in JSON versus the 7.33 MB audit baseline.
The explicit evidence profile remains capped by the same work and output ceilings.

**Acceptance criteria:**

- [x] The default compact profile emits selected snippets and bounded summaries only. Exhaustive symbols,
      edges, findings, omissions, and commit evidence require an explicit evidence profile or pagination.
- [x] `--map-tokens` counts every data-dependent compact-map field and never reports an estimate above the
      requested budget. The fixed envelope is documented and bounded; property tests cover tiny budgets.
- [x] Every collection exposes `total`, `returned`, and `truncated` plus a reason. Repeated ambiguity is grouped
      and sampled rather than emitted once per reference occurrence.
- [x] Inventory prunes `.git`, ignored build/dependency/vendor directories, and nested repositories before
      descent. Hidden, non-ignored source remains eligible, and ignored omissions use bounded counts/samples.
- [x] Reviewed limits cover file count/bytes, total bytes, syntax depth, symbols, candidate fan-out, edges,
      findings, commits, history evidence, elapsed work, and output. Oversize/binary/deep inputs become typed
      omissions; syntax traversal cannot overflow the call stack.
- [x] Focused history uses an operation-aware streaming pass and computes tree diffs only when the requested
      evidence and path scope require them.
- [x] Long scans expose concise TTY-only stderr progress, stay quiet when non-interactive, honor interruption
      promptly, and cannot leave a partial cache entry or JSON document described as successful.
- [ ] Benchmarks on Codeplat itself, a large ignored tree, high-ambiguity sources, and 10k/100k-commit fixtures
      enforce documented latency and output ceilings. The eight-file self-map is orders of magnitude smaller
      than the 7.33 MB audit baseline.

**Verification:**

- [x] Assert byte size, estimated tokens, returned counts, and truncation metadata for compact and evidence profiles.
- [x] Run resource fixtures under CI-friendly time and memory ceilings and confirm useful partial output;
      the integration suite covers oversized and binary source inputs with typed omissions.
- [x] Test early pipe closure; a broken pipe terminates quietly rather than becoming exit 70.
- `cargo fmt --check`
- `cargo test --all-features`
- `cargo clippy --all-targets --all-features -- -D warnings`

## 12. Make machine reports reproducible and enforceable

**Priority:** P0 release blocker

Give agents enough typed provenance to decide whether a report is comparable and usable,
and turn schema version 1 into a real compatibility contract.

## 13. Correct and strengthen Git-history evidence

**Priority:** P0 correctness for scope/provenance; P1 for signal enrichment

**What to build:** Make every history result obey its advertised scope and improve the article-derived
signals without hiding their original evidence or caveats.

**Blocked by:** Tickets 2 and 12

**Implementation status (2026-07-16):** Scoped commit envelopes and activity, committed-HEAD `.mailmap`
canonicalization, email-redacted compact contributor output, word-aware/substring keyword policies,
matched-term evidence, current-HEAD size-normalized churn, and bounded focused scans are implemented.
Rename continuity is explicitly reported as unavailable rather than silently implied.

**Acceptance criteria:**

- [x] `history activity PATH` and `commits_seen` count only commits affecting the selected scope. Any
      intentionally repository-wide section carries a separate explicit scope instead of inheriting `PATH`.
- [x] Contributor concentration applies `.mailmap` by default, records raw-to-canonical provenance, handles
      missing/case-varied identity, and omits email from compact output unless explicitly requested.
- [x] Bug/firefighting evidence records the matched term and uses word-aware matching by default, with an
      explicit substring mode for compatibility; `fixture`, `prefix`, and `debug` are negative tests.
- [x] Size-normalized churn is reported beside—not instead of—absolute churn, with the size basis and zero/
      generated/binary-file behavior explicit.
- [x] Rename-aware continuity is either implemented with evidence and limits or reported as unavailable;
      a renamed hotspot never silently appears to have no earlier history.
- [x] Focused operations decode and diff only required data, preserve deterministic ties, and expose bounded
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

**Implementation status (2026-07-16):** Language-scoped candidate indexing, explicit same-file and import/module
evidence, typed visibility/evidence tags, confidence-aware edges, stable candidate groups, grouped ambiguity,
preaggregated incoming counts, per-language parser/query reuse, and the bounded `explain` command are implemented.
Raw tags remain available while locals, fields, imports-as-definitions, generic identifiers, and unresolved
property references are kept out of graph centrality.

**Acceptance criteria:**

- [x] Candidate matching is constrained by language plus import/module/scope information where available.
      Same-file and cross-language bare-name matches do not affect centrality without explicit evidence.
- [x] Each retained edge carries a resolution reason and confidence tier. Candidate groups are interned,
      deduplicated by path/symbol identity, fan-out capped, and repeated ambiguity findings grouped.
- [x] Centrality excludes or sharply discounts locals, fields, generic identifiers, imports masquerading as
      definitions, and unresolved property names; raw tags remain available in the explicit evidence profile.
- [x] `codeplat explain <PATH-OR-SYMBOL>` decomposes focus, history, landmark, graph, ranking, ambiguity, and
      omission contributions without describing lexical evidence as semantic truth.
- [x] Every first-class language has positive/negative conformance fixtures for definitions, references,
      imports/aliases, visibility, nesting, overloads/generics as applicable, malformed input, and conventional
      extensionless entry files. The declared Java field capability and empty-map provenance are accurate.
- [x] Parsers and compiled queries are reused per language, incoming counts are preaggregated, and the focused
      graph regression tests prevent candidate-copy and edge-rescan amplification.

**Verification:**

- [x] Assert graph candidates, confidence reasons, grouped ambiguity, and ranking against hand-reviewed mixed-language fixtures.
- [x] Run the self-map and verify core analyzer files outrank repeated field names under the default profile.
- [x] Record bounded output and candidate fan-out behavior in the compact/evidence regression suite.
- [x] `cargo fmt --check`
- [x] `cargo test --all-features`
- [x] `cargo clippy --all-targets --all-features -- -D warnings`

## 15. Add repository landmarks and project topology

**Priority:** P1 post-v1 navigation feature

**What to build:** Help users choose the first non-source files and project roots to read before symbol-level
navigation, using bounded presence-based evidence rather than speculative framework semantics.

**Blocked by:** Tickets 8, 11, and 12

**Acceptance criteria:**

- [x] The integrated briefing identifies README/contributor/agent instructions, manifests and lockfiles,
      workspace/package roots, build/task entry points, test roots, CI, ownership files, licenses, submodules,
      and nested repositories within scope.
- [x] Every landmark includes a stable kind, path, detection reason, project-root association, and worktree
      state. Unknown or conflicting files remain `unknown`; contents are read only within shared size/safety limits.
- [x] Monorepo output groups landmarks and source recommendations by detected project root without inventing
      dependencies between packages.
- [x] Compact output returns a bounded, prioritized landmark set plus totals; the evidence profile can expose
      the complete inventory through pagination.
- [x] Focus paths and explicit exclusions apply consistently, and submodules/nested repositories are boundaries
      unless the caller explicitly requests recursive analysis.

**Verification:**

- [x] Exercise Rust, Node/TypeScript, Python, Ruby, Java, .NET, and mixed-monorepo landmark fixtures.
- [x] Assert AGENTS/README/build/test/CI/ownership priority and safe nested-repository/submodule behavior.
- [x] `cargo fmt --check`
- [x] `cargo test --all-features`
- [x] `cargo clippy --all-targets --all-features -- -D warnings`

## 16. Compare repository orientation across revisions

**Priority:** P1 post-v1 navigation feature

**What to build:** Let a user or agent compare two explicit repository states and see bounded changes to the
evidence that should alter what they read next.

**Blocked by:** Tickets 12, 14, 15, and 17

**Acceptance criteria:**

- [ ] `codeplat compare --base <REV> [--head <REV|worktree>] [PATH]` resolves and records both states without
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
      capabilities Codeplat needs. `cargo tree -e features` is recorded, and executable/network/credential/
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
- [ ] The default Markdown and JSON briefings pass a manual usefulness review on a small project, Codeplat,
      and a mixed-language monorepo. No known P0 finding is waived without a recorded rationale and expiry.

**Verification:**

- `cargo fmt --check`
- `cargo test --workspace --all-features`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo doc --workspace --all-features --no-deps`
- `cargo package`
- Inspect `cargo tree -e features -i gix`, packaged contents, release artifact checksums, completions, and man pages.
