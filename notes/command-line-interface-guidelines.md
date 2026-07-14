---
title: Command-line interface design guidelines
sources:
  - https://clig.dev/
author: Aanand Prasad, Ben Firshman, Carl Tashian, and Eva Parish
captured: 2026-07-14
tags:
  - cli
  - user-experience
  - automation
---

## Summary

A good modern CLI is human-first and discoverable while preserving the stable streams, exit codes,
and explicit behavior required for automation.

## Source Boundary

- **Command Line Interface Guidelines:** Principles and concrete conventions for conventional,
  composable command-line programs; it does not prescribe a language or framework.

## Key Ideas

- **Humans and programs both matter:** Composability through stdout, stderr, exit codes, signals, and
  structured output can coexist with clear, empathetic interactive use.
- **Discovery is a core feature:** Concise no-argument help, thorough `--help`, examples, consistent names,
  and suggested next commands make the interface learnable.
- **Output is a contract:** Keep successful human output brief, reserve stdout for primary and machine-readable data,
  reserve stderr for diagnostics, and provide JSON when the data is structured.
- **Automation must not be surprised:** Disable decoration outside a TTY, make non-interactive behavior explicit,
  and never silently reinterpret an unknown command or abbreviation.

## What a Well-Behaved CLI Does

- Provides `-h`/`--help`, clear top-level and subcommand help, version information, examples, and a feedback path.
- Returns zero on success and meaningful non-zero codes for failures.
- Emits the requested result to stdout; sends errors, warnings, and progress to stderr.
- Supports `--json` for structured results and avoids ANSI color or animation when the corresponding stream is not an interactive terminal.
- Honors `NO_COLOR`, `TERM=dumb`, and an explicit `--no-color`; uses color sparingly and meaningfully.
- Validates input early, explains failures plainly, avoids stack traces by default, and suggests corrections without automatically changing a command's meaning.

## How It Works

### Interface shape

Use subcommands when related capabilities share configuration, help conventions, and storage. Keep verbs and flag
names consistent. Avoid catch-all subcommands and automatic abbreviation because either creates future compatibility
traps.

### Human and machine modes

Terminal presentation can be concise, colored, and oriented around next steps. Machine mode should be deterministic
structured output. Each output stream is decided separately: a piped stdout does not imply stderr must lose useful
diagnostics or color.

### Interactivity and safety

Only prompt when stdin is a TTY. `--no-input` must disable prompts. Any dangerous action needs an explicit confirmation
or force flag, though read-only inspection generally should not prompt.

### Configuration

Apply configuration with this precedence: flags, environment variables, project configuration, user configuration,
then system configuration. Put per-invocation choices in flags and stable project-wide choices in version-controlled
configuration.

## Claims & Evidence

### Stream discipline makes tools composable

The guide recommends primary and machine-readable results on stdout, with errors and messages on stderr,
so pipelines receive data rather than status chatter.

Confidence: high; it follows established terminal and UNIX interfaces.

### Explicit commands and aliases preserve compatibility

The guide warns that catch-all dispatch and arbitrary subcommand prefixes make later command additions breaking changes for scripts.

Confidence: high; this follows directly from reserving future command names.

## Important Terms

| Term                 | Meaning                                                                                              |
| -------------------- | ---------------------------------------------------------------------------------------------------- |
| TTY                  | An interactive terminal; its presence distinguishes conversational use from a pipe or script.        |
| stdout               | The stream for a command's primary result, including structured data.                                |
| stderr               | The stream for diagnostics, warnings, errors, and progress.                                          |
| `NO_COLOR`           | A convention: a non-empty environment value asks a command not to emit ANSI color.                   |
| Catch-all subcommand | A dispatcher that interprets arbitrary unknown text as a command; it prevents safe future expansion. |

## Lessons To Reuse

- Design JSON and terminal rendering from one result model instead of parsing formatted text into a machine interface later.
- Make no-argument behavior intentionally teach a first useful action.
- Treat help text, errors, and exit status as stable public interfaces that need tests.

## Questions for Review

- Why should diagnostics not share stdout with JSON results?
  - A downstream program expects stdout to contain only the requested data; interleaved messages corrupt it.
- When should a CLI prompt?
  - Only on an interactive stdin and never as the sole way to provide required input.
- Why are arbitrary command prefixes dangerous?
  - They reserve every matching future command name and can silently change a script's behavior after an upgrade.
- Which conditions should disable colors?
  - A non-TTY destination, non-empty `NO_COLOR`, `TERM=dumb`, or an explicit no-color option.

## Connections

- Related ideas: UNIX composition, API compatibility, accessible design, progressive disclosure.
- Related sources: `NO_COLOR` convention and XDG Base Directory Specification.
- Contradictions or tensions: rich terminal output improves scans for people, while scripts need strictly undecorated data;
  separate modes resolve the tension.
- Useful applications: developer tools, CI automation, agent tool calls, and maintenance utilities.

## Open Questions

- What JSON schema-versioning policy best preserves compatibility as commands evolve?
- Which non-zero exit codes deserve stable, documented meanings beyond generic failure?
- Which common flows benefit from a terse default command versus requiring an explicit subcommand?

## Takeaways

- Human-friendly and automation-friendly CLI behavior are complementary design goals.
- Stable streams, exit codes, and explicit command names are compatibility guarantees.
- Help, errors, colors, and JSON output all need deliberate, testable contracts.
