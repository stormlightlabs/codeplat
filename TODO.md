# Tickets: Codeplat V1

## Release blockers

- Generated, vendored, and minified source can still consume analysis limits and degrade recommendations.
- Lua and Zig do not yet have first-class structural-map support.
- Scale benchmarks do not yet enforce latency/output ceilings for ignored trees, high ambiguity, and deep history.
- The configured Linux, macOS, Windows, Rust 1.85, and dependency-policy jobs need a green release-candidate run.

## Completed foundation

Earlier completed tickets established the CLI/report contract, five history signals, seven language families,
cache modes, bounded lexical maps, the integrated briefing, the evidence-backed default reading plan,
hostile-repository containment, report provenance/schema fixtures, history correctness, explainable lexical
evidence, repository landmarks/topology, and the concise default history briefing.

The packaging work added metadata/licensing, minimal `gix` features, dependency policy, cross-platform/MSRV CI,
checksummed artifacts, generated completions/man pages, and release documentation.

## 18. Build the default repository reading plan

**What to build:** Make `codeplat [PATH]` lead with a practical, evidence-backed sequence of files to read rather
than a flat ranked winner and long diagnostic sections. The same typed reading plan must be available in JSON.

**Blocked by:** None - can start immediately

**Acceptance criteria:**

- [x] The shared report model represents ordered recommendations with purpose, path, project root, reason,
      evidence kinds, confidence, and relevant limitations.
- [x] A useful repository receives 5–10 unique recommendations when sufficient evidence exists, using applicable
      `start_here`, `architecture`, `runtime`, `tests`, and `supporting_context` groups.
- [x] Selection combines landmarks, project topology, qualified lexical ranking, explicit focus, and bounded
      history overlap without adding framework guesses or opaque score-only explanations.
- [x] Monorepo plans cover relevant project roots without overriding explicit focus; omitted roots and short plans
      are explained instead of padded.
- [x] Markdown leads with repository overview and the reading plan. JSON ordering is deterministic and extends the
      schema compatibility corpus without changing existing field meanings.
- [x] `map` and `explain` remain focused evidence tools and existing callers do not need to parse Markdown.

## 19. Make the default history briefing concise and useful

**What to build:** Replace exhaustive history tables in the default Markdown briefing with 3–5 distinct,
evidence-backed observations, while preserving detailed history commands and machine evidence.

**Blocked by:** None - can start immediately

**Acceptance criteria:**

- [x] The default briefing selects at most five distinct observations across churn, contributors, bug overlap,
      activity, and firefighting evidence; empty/noisy signals do not fill a quota.
- [x] Every observation states the supporting paths/counts/window and the existing caveat needed to avoid a
      quality judgment.
- [x] Exhaustive churn paths, contributor tables, activity months, and commit lists are absent from default
      Markdown and remain available through `history`, its focused subcommands, JSON, or `--profile evidence`.
- [x] The default Markdown briefing is materially shorter and places the reading plan before history detail.
- [x] Focused history Markdown/JSON behavior and schema compatibility remain intact.

## 20. Keep generated, vendored, and minified source out of the default plan

**What to build:** Classify low-value generated/vendor/minified source before parsing so it cannot consume default
analysis limits, recommendations, or actionable quality status.

**Blocked by:** None - can start immediately

**Acceptance criteria:**

- [x] Deterministic rules cover conventional dependency/vendor/build directories, generated filenames/markers,
      source maps, `.min.*` files, and a documented bounded minification heuristic.
- [x] Compact mode records typed totals and bounded samples but does not parse or recommend classified files,
      including tracked files.
- [x] Exact focus paths can include classified text files within existing safety and
      resource limits; unsafe, binary, and oversize protections are never overridden.
- [x] Classification reason and override state are visible in Markdown/JSON and deterministic across runs.
- [x] A vendor/minified file cannot make an otherwise complete reading plan partial or consume the per-file symbol
      budget.
- [x] Maintained source that resembles generated output has negative fixtures and an explicit recovery path.

**Verification:**

- Exercise tracked/untracked vendor directories, generated markers, minified JS, large generated Rust, false
  positives, focus overrides, and evidence mode through the compiled CLI.
- Assert lower analyzed-byte/symbol counts and unchanged maintained-source recommendations.
- Run hostile-path/cache regressions to prove classification does not bypass safe reads.
- Run the standard workspace format, test, and Clippy commands.

## 21. Add first-class Go maps

**What to build:** Give Go repositories the same bounded structural-map and reading-plan support as existing
first-class languages.

**Blocked by:** None - can start immediately

**Acceptance criteria:**

- [x] A reviewed upstream Go Tree-sitter grammar and versioned query pack are registered with minimal features.
- [x] Definitions cover packages, functions, methods, types, structs, interfaces, fields, constants, variables,
      and common test declarations with accurate locations and declaration context.
- [x] References/import evidence covers imports and aliases, calls, type uses, selectors, and same-package evidence
      without claiming type resolution.
- [x] Exported capitalization, receiver scope, package context, `_test.go`, malformed input, generics, duplicate
      names, and ambiguous candidates have positive and negative conformance fixtures.
- [x] Go participates in mixed-language ranking, capabilities, provenance, cache identity, limitations, Markdown,
      and JSON; the generic reading-plan contract can consume its ranked evidence without language-specific logic.
- [x] README/help/roadmap language lists match implemented support.

**Verification:**

- Run `codeplat`, `codeplat map --json`, focused maps, and `capabilities --json` against Go and mixed fixtures.
- Assert definitions, references, visibility, evidence reasons, ambiguity, ranked evidence, and partial behavior.
- Run the standard workspace checks plus `cargo package --locked`.

## 22. Add first-class Lua maps

**What to build:** Give Lua repositories bounded structural maps that handle common module patterns without
pretending dynamic name resolution is semantic.

**Blocked by:** None - can start immediately

**Acceptance criteria:**

- [ ] A reviewed upstream Lua Tree-sitter grammar and versioned query pack are registered with minimal features.
- [ ] Definitions cover local/global functions, method syntax, local variables, table fields, assignments, and
      returned module tables where syntax provides reliable evidence.
- [ ] References/import evidence covers calls, identifiers, field access, and literal `require` module paths;
      dynamic `require` and metatable behavior are explicit limitations.
- [ ] Dot/colon methods, nested scopes, module-return patterns, malformed input, duplicate names, and common
      extensionless Lua entry files have positive and negative conformance fixtures.
- [ ] Lua participates in mixed-language ranking, capabilities, provenance, cache identity, Markdown, and JSON
      without cross-language bare-name fan-out; the generic reading-plan contract can consume its ranked evidence.
- [ ] README/help/roadmap language lists match implemented support.

**Verification:**

- Run default, JSON map, focused, and capabilities commands against Lua and mixed fixtures.
- Assert module evidence, scopes, declaration context, ambiguity, limitations, and ranked evidence.
- Run the standard workspace checks plus `cargo package --locked`.

## 23. Add first-class Zig maps

**What to build:** Give Zig repositories bounded structural maps for declarations, imports, tests, and public API
orientation.

**Blocked by:** None - can start immediately

**Acceptance criteria:**

- [ ] A reviewed upstream Zig Tree-sitter grammar and versioned query pack are registered with minimal features.
- [ ] Definitions cover functions, variables/constants, container types, fields, tests, and public declarations
      with accurate locations, scopes, and declaration snippets.
- [ ] References/import evidence covers calls, identifiers, field access, type uses, and literal `@import`
      paths; compile-time and inferred semantics remain explicit limitations.
- [ ] `pub`, nested containers, anonymous containers, error unions, generics/comptime syntax, malformed input,
      duplicate names, and test blocks have positive and negative conformance fixtures.
- [ ] Zig participates in mixed-language ranking, capabilities, provenance, cache identity, Markdown, and JSON;
      the generic reading-plan contract can consume its ranked evidence without language-specific logic.
- [ ] README/help/roadmap language lists match implemented support.

**Verification:**

- Run default, JSON map, focused, and capabilities commands against Zig and mixed fixtures.
- Assert definitions, references, visibility, import evidence, limitations, and ranked evidence.
- Run the standard workspace checks plus `cargo package --locked`.

## 24. Make compact quality and strict policy actionable

**What to build:** Separate expected compact projection from conditions that make a briefing materially unsafe or
misleading, so normal bounded output does not look like a failed analysis.

**Blocked by:** Tickets 18 and 20

**Acceptance criteria:**

- [x] Collection summaries continue to expose totals, returned counts, truncation, and reasons for every bounded
      collection.
- [x] Top-level quality distinguishes expected profile projection from resource exhaustion, stale evidence,
      missing history, unsafe paths, unsupported relevant source, and partial recommended/focused files.
- [x] Unsupported or partial files outside the reading plan remain discoverable but do not automatically poison a
      useful compact briefing.
- [x] `--strict` emits the typed report and exits 5 only for documented actionable degradation; evidence mode and
      focused commands apply the same policy consistently.
- [x] The schema and golden corpus preserve existing meanings or deliberately advance the schema version if that
      cannot be done additively.
- [x] Markdown limitations are concise, prioritized, and include the next useful command.

**Verification:**

- Add a quality-policy matrix covering compact projection, hard work limits, stale manual cache, missing objects,
  irrelevant partial vendor files, partial recommended files, unsupported relevant source, and unsafe paths.
- Assert report fields, strict exit status, stdout/stderr policy, and compatibility documents.
- Run the full compiled CLI suite in compact/evidence and strict/non-strict modes.
- Run the standard workspace format, test, Clippy, and docs commands.

## 25. Enforce V1 scale and usefulness gates

**What to build:** Turn the current resource ceilings and subjective release review into repeatable evidence that
the default briefing stays fast, bounded, and useful on realistic repositories.

**Blocked by:** Tickets 19, 20, 21, 22, 23, and 24

**Acceptance criteria:**

- [ ] CI-friendly benchmarks cover Codeplat, a large ignored/vendor tree, high-ambiguity sources, and synthetic
      10k/100k-commit histories with documented latency and output ceilings.
- [ ] Benchmark failures identify the exceeded work/output dimension and do not depend on network access or
      private repositories.
- [ ] A reusable release-review rubric checks reading-plan count/coverage/reasons, concise history, actionable
      quality, stdout/stderr, and manual usefulness without reducing the result to one opaque score.
- [ ] The release binary is rerun across the available first-party project corpus; only aggregate outcomes and
      reproducible public/synthetic regressions are retained.
- [ ] Small-project, Codeplat, and mixed-monorepo Markdown briefings pass recorded human review with no known P0
      usability or correctness finding waived without rationale and expiry.

**Verification:**

- Run the benchmark harness under documented CI time and output ceilings.
- Run every fixture in Markdown/JSON, compact/evidence, strict/non-strict, and relevant cache modes.
- Run the release-binary project sweep and inspect aggregate failures, recommendation coverage, partial/unsupported
  causes, maximum output, and unexpected stderr.
- Run all standard workspace and package checks.

## 26. Ship V1

**What to build:** Produce the supportable V1 release only after every product, safety, performance, packaging,
and platform gate is green.

**Blocked by:** Ticket 25

**Acceptance criteria:**

- [ ] The default reading plan, concise history, generated/vendor handling, quality policy, and all ten language
      families match README/help/schema/capabilities documentation.
- [ ] Formatting, all-feature workspace tests, Clippy with warnings denied, docs, schema compatibility, package
      verification, dependency policy, minimal `gix` features, generated assets, and benchmark gates pass.
- [ ] Linux, macOS, Windows, and Rust 1.85 CI jobs pass on the release candidate.
- [ ] Checksummed release archives are reproducible from the committed lockfile and install/uninstall/cache cleanup
      instructions are verified.
- [ ] No release blocker remains in this file. Any waived P0 finding has an owner, rationale, and expiry recorded
      before release.

**Verification:**

- `cargo fmt --all -- --check`
- `cargo check --workspace --all-targets --all-features`
- `cargo test --workspace --all-features`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps`
- `cargo package --locked`
- `cargo release-assets` followed by generated completion/man-page existence checks
- Inspect CI, benchmark results, package contents, feature tree, dependency policy, and artifact checksums.

## Deferred after V1

- Revision comparison between repository states.
- F#, Elixir, C, and C++ language support.
- Semantic-provider and framework-specific recommendations.

## Frontier

- Ticket 22: Add first-class Lua maps.
- Ticket 23: Add first-class Zig maps.
