# AGENTS.md

## Project

fast-publicodes is an npm package with the same API and behaviour as `publicodes` 1.x, whose engine is reimplemented in Rust for speed. It targets Node.

## Architecture

- `crates/engine`: the publicodes engine in pure Rust, unaware of JavaScript. `engine.rs` drives rules through `parse` (raw rules to nodes, inline mechanisms expanded), `resolve` (references), `replace` (`remplace` and `rend non applicable`), `types` (inference) and `evaluate`; it mirrors the `publicodes` source file by file.
- `crates/node`: napi-rs binding exposing the engine to Node; built into `native/` (addon plus generated JS loader and types).
- `src/`: the TypeScript package, wrapping the binding behind the `publicodes` API and types; compiled to `dist/`.

Rules and evaluation state live in Rust; calls cross the binding in bulk.

Only what an agent needs to place a change correctly. The why is in ADRs.

## Layout

| Path | Holds |
| --- | --- |
| `crates/` | Rust crates: `engine` and `node` (binding) |
| `src/` | TypeScript package source |
| `test/` | Vitest tests, mirroring `src/` |
| `test/compat/` | Comparison against `publicodes`; suites (rules and situations) in `suites/` |
| `bench/` | Benchmarks against `publicodes` |
| `docs/` | Documentation and ADRs |

One row per top-level directory; deeper only where the tree alone would not tell an agent where a change belongs.

## Commands

| Command | Purpose |
| --- | --- |
| `pnpm install` | Install dependencies |
| `pnpm build:native` | Build the debug addon into `native/`; rerun after Rust changes, before TypeScript tests |
| `pnpm build` | Release addon and compiled package |
| `pnpm lint` | Clippy, rustfmt check, Biome, type check |
| `pnpm format` | Format Rust and TypeScript |
| `cargo test -p <crate> [filter]` | Rust tests of one crate, optionally filtered by name |
| `pnpm bench` | Release build, then benchmarks against `publicodes` on modele-social |
| `pnpm test [path] [-t <name>]` | Vitest, optionally scoped to a file or directory and to tests whose name matches, e.g. a suite |

Design each command for who runs it: agent-facing commands run without interaction, accept a scope where one applies and print only what an agent needs. Agent-facing commands are listed here, human-facing ones in README.md, shared ones in both.

## Economy

**Context.** Everything committed to this repository is read by every agent that works here later: code, comments, docs, tests, ADRs and this file. Every token that carries no necessary information costs money and dilutes attention on the information that matters. Each kind of knowledge has one home: the **why** in ADRs, the **what** in documentation, the **how** in the code.

**Time.** Every check, run and read costs time. Do each once, at the narrowest scope that proves what you need, and skip anything your change cannot affect.

## Git

Never run a command that alters git history or the remote (commit, amend, rebase, merge, reset, cherry-pick, push, tag, branch deletion) without explicit approval from the user for that specific command. Approval covers one action and never carries over to the next.

## Code

- Follow industry best practice. No over-engineering, no quick fixes.
- The project will grow a lot. Structure it so the next change is easy, not so future features are pre-built. Be smart, not exhaustive.
- Keep the tree, the files and the code organised and clear without being verbose. Code explains the how by itself.
- When the change you are making tips the existing structure into being less maintainable, refactoring is part of that change. Do it now.
- Leave nothing unused: no dead code, no dormant logic, no remnant of a replaced approach. The codebase reflects exactly what it does today.

## Verification

Never guess: run the code and observe. Verify the full behaviour of what you change, including its callers. Never deliver untested work.

## Tests

Write tests that block regressions. Each test costs context, maintenance and pipeline time, so every test must protect a behaviour that matters and run fast; nothing for the sake of coverage.

Lay out tests so one module's or feature's tests run alone through a scoped command. Run only the tests for your change; the full suite belongs to the pipeline.

## Comments

A comment is re-read by every agent that passes through its file and costs context each time. Once the why, the what and the how have their homes, very few comments are legitimate.

- Comment only what cannot be understood from the code itself.
- Describe the code as it is now: no former behaviour, no plans.
- Keep comments within the scope where they appear: a comment inside a function describes only that function; a file-level comment describes only that file. Never reference code or behaviour outside that scope, including as a reason for the local implementation.
- What is in your context is not what is in the code. Never record a decision abandoned along the way, an alternative you considered, or what your change replaced: after replacing A with B, do not comment that A is not used.
- Create, update and delete comments in the same change that alters the code they describe.
- If you hesitate over a comment, leave it out.

## Documentation

Documentation explains the what, in plain human English, structured for a reader. Most work does not need documentation. Add documentation only when the information is important for future readers to understand or use the project; treat every addition, even a word, as a deliberate choice. Create a new document only when that information cannot be covered clearly in an existing one.

- Describe the present state: no history, no plans.
- What is in your context is not what the reader needs. Never document a decision abandoned along the way, an alternative you considered, or what your change replaced: after replacing A with B, document B and never mention A.
- Keep all documentation at one uniform level of detail. What you have just built looms large in your context; do not detail it more than the rest.
- Create, update and delete documentation in the same change that alters what it describes.
- If you hesitate over a sentence, leave it out.

Every document except ADRs is listed here. Read one only when the task needs it.

| Document | Covers |
| --- | --- |
| `docs/testing.md` | Compatibility suites, fuzzing and turning fuzz failures into regressions |

## ADR

Record every architecturally significant decision (structure, dependencies, interfaces, non-functional trade-offs) in `docs/decisions/NNNN-short-title.md`, NNNN incrementing. An ADR holds the why: exactly what is needed to understand the decision, nothing more.

```markdown
# NNNN. Title

Status: accepted | superseded by NNNN

## Context
## Decision
## Consequences
```

Never edit an accepted ADR beyond its status line; a new ADR supersedes it.

## Skills

Create a skill at `.agents/skills/<name>/SKILL.md` only for a task agents will repeat frequently over the life of the repository and that a written procedure makes faster or safer. Keep it current; delete it when the task disappears.

## AGENTS.md and README.md

You may edit this file only to: refresh the project description when it is no longer accurate (never to add detail); update Architecture and Layout when a change alters them; add, update or remove commands; add, update or remove a documentation entry. Everything else here is owned by the user.

README.md is the landing page for humans: description, installation, usage, commands, in clear sections. Keep it accurate and strictly minimal.
