# Agent Guide

AppyTappy is a Windows background app that runs automations when another
application reaches a state, such as a certain number appearing in a region of
its window or an icon becoming visible. Read
`CONTEXT.md` first if it exists. Then read the pages in `docs/` that own the
change before changing architecture, domain language, or enforcement.

## Commands

`mise run` is the entry point for project workflows; tasks wrap the toolchain.
Run `mise tasks` and read the definitions of the tasks you use. Extend an
existing task before you add one.

- `mise install` installs the pinned tools. When an install needs network, run
  it and report that.
- `mise run standards` applies the available autofixes.
- `mise run standards:check` is the CI gate; run it before handoff.

Run `git`, `rg`, `tokei`, and standalone scripts directly. Use native commands
for focused diagnosis and the mise gate for final verification. Use
`mise exec -- <command>` only when a command needs a pinned tool or environment
and no task covers it.

## Platform

AppyTappy ships for Windows only. CI runs these jobs from
`.github/workflows/quality.yml`: `quality` runs `mise run standards:check` on
Linux and is the required gate, `windows` runs `rust:fmt:check`,
`rust:lint`, `rust:doc`, and `rust:test:members` on Windows, and `mutants`
shares the full mutation sweep with `quality` on pushes to main and manual
runs.

- Inline mise tasks are POSIX `sh`. On a Windows workstation, run mise from
  Git Bash and set this in the global mise config
  (`~/.config/mise/config.toml`), because mise ignores it in project config:

  ```toml
  [settings]
  windows_default_inline_shell_args = "sh -c -o errexit"
  ```

- `rust:policy` and the mutation tasks need `setsid`, which Windows and macOS
  lack, so `standards:check` runs on Linux: in CI or WSL. The root package's
  policy tests compare POSIX paths, so `rust:test` runs on Linux or macOS. On
  Windows, run `rust:fmt`, `rust:lint`, `rust:doc`, `rust:test:members`, and
  the `md:` tasks.
- `mise lock` records every platform in `lockfile_platforms`; commit
  `.config/mise/mise.lock` after a tool change.

## Leave It Current

The repository holds current contracts: the code, the tests that defend it, and
the docs that explain it. Git holds history. Unless the project has released
users or a documented external contract, it makes no compatibility promise: no
shims, aliases, deprecations, dual code paths, format migrations, or
version-numbered names.

- Supersede rather than accumulate. Rename in place, replace a format, and
  delete the old reader with its fixtures and tests in the same change.
- A change that makes something obsolete removes it: functions and exports
  nothing calls, config keys nothing reads, tests of retired behavior, and docs
  that describe it. Finishing a feature includes removing the scaffolding that
  built it.
- Shipped code has no test-only branches, alternate rules, or test-only
  diagnostic surfaces. Narrow test-build instrumentation and fault injection,
  such as `#[cfg(test)]` code, are fine when they leave production semantics
  unchanged, and so is a seam that production also uses, such as a seed or an
  event log that feeds replays.
- Proof lives in the gate. Back each lasting claim with a test in
  `standards:check` or a task that checks a stated target. A one-off
  diagnostic leaves with the question it answered, and its result goes in the
  commit message. Do not build evidence scaffolding, such as bundles,
  manifests, or checks that repeat a guarantee the gate or toolchain already
  gives. Keep focused tests for the scripts and measurements the workflow
  relies on.
- Prefer native toolchain guarantees, remove duplicate checks, and measure the
  gate's routine cost before you add verification.
- Keep task notes in `.scratch/` and run output in `artifacts/`. Both stay
  untracked and never become a second policy source. Do not commit briefs,
  plans, handoffs, research, reports, or dated audits.
- `mise run hygiene` enforces the mechanical part and prints the tree's size by
  category, so growth shows in every handoff. Explain growth that the change
  does not account for.

## Principles

- Complexity is the enemy; see [grugbrain.dev](https://grugbrain.dev/). Prefer
  boring, obvious code with local state, explicit side effects, and direct
  data flow. Add structure after the shape is visible: small duplication beats
  premature indirection. Keep ecosystem-idiomatic strictness where it prevents
  concrete failures. Judge each tool, rule, test, abstraction, service, and doc
  by the uncertainty, defects, or manual work it removes, including its runtime
  and maintenance cost, and remove ceremony that cannot justify that cost.
- Build deep modules with small interfaces. Change a feature's behavior,
  state, and tests together, and put tests where the ecosystem expects them.
  Split a module that holds more than one cohesive responsibility. Fold
  shallow wrappers and forwarding helpers into their callers. Import through
  the owning module's intended interface, and delete re-exports that only
  preserve obsolete paths.
- Respect Chesterton fences. Understand why code exists before deleting or
  replacing it; once it serves no current contract, delete it.
- Name things for what they do in the domain, in `CONTEXT.md` vocabulary.
- Comments explain a current invariant, a non-obvious algorithm, a trust
  boundary, or a public interface. Delete prose that retells an old
  implementation or narrates what names and types already say.
- Write good-enough idiomatic code that is easy to reason about, and optimize
  only what a measurement shows matters. Measure before a choice that is
  expensive to reverse.

## Editing

- Repair the cause of a failure; a retry does not turn an unexplained
  divergence into a pass.
- Suppress a lint only on the smallest item, with a stated reason. If the
  language supports it, use a suppression that fails when it becomes
  unnecessary.
- Paths that `.gitignore` ignores are generated or local. When generated output
  is wrong or stale, fix its source or task and regenerate.

## Documentation

- Docs explain current behavior and design constraints. Update the owning page
  when the contract changes. State each policy once and link to it elsewhere.
- Executable config is the source of truth; docs point at it rather than
  restate it.
- Keep the workflows, contracts, and current state discoverable from local
  files, so a fresh agent can find them.
- Write in the present tense. Do not date text or narrate history, as in "now
  uses", "no longer", "previously", or "the new parser".
- `docs/` holds current pages and their images. Record owner decisions on one
  current page and replace a decision when it changes.
- Follow the
  [Google developer documentation style guide](https://developers.google.com/style)
  for docs and commit prose.
- `md:standards:check` runs markdownlint, Oxfmt, an offline local link check,
  and typos. Inline `markdownlint` comments are disabled: record an exception
  as a narrow `overrides` entry in `.markdownlint-cli2.jsonc` with its reason.
  Add intentional domain words to `typos.toml`.
- Give every code fence a language identifier.

## Testing

- A test earns its place by defending behavior, an invariant, a failure mode,
  or a trust boundary that types and the gate do not already enforce. Do not
  test trivial getters, constructors, enum declarations, framework behavior,
  config values copied onto objects, or incidental numbers.
- A meaningful assertion fails for a concrete production defect and stays
  valid through behavior-preserving refactors. Show that a new or changed
  assertion can fail: for a bug, reproduce it with a failing regression test
  before the fix when practical; otherwise inject a fault for its claim.
  Mechanical moves need no new evidence.
- Prefer one focused test at the narrowest stable interface over duplicated
  examples. Keep test setup smaller than the behavior it protects.
- Test counts and coverage percentages are not goals.

## Rust

`Cargo.toml`, `clippy.toml`, and `deny.toml` own executable policy;
`.config/mise/conf.d/rust.toml` owns commands. Read
[`docs/rust.md`](docs/rust.md) for setup, scanner limits, and mutation
operation before changing enforcement.

### Work and verification

Use `mise run rust:...` for development. Run `rust:lint` and `rust:test` for
source changes; add `rust:doc` for public API changes.
`rust:standards` formats code. Refresh `Cargo.lock` through `rust:lock` after
dependency changes and commit it.

`rust:mutants` runs the full sweep. `rust:mutants:diff` mutates only changed
code; its base defaults to `origin/main`, then local `main`, and can be set
with `MUTANTS_BASE_REF`. Include new files in the Git diff before running it.
`rust:standards:check` runs the lane that `MUTANTS_LANE` names, `full` by
default; [mutation testing](docs/rust.md#mutation-testing) says which lane CI
runs.
Before handoff, run `mise run rust:standards:check`; report skipped checks and their
reasons.

### Ownership and state

This application profile favors one owner for state, pure `(state, event)`
transitions, and messages to the owner. Libraries, runtimes, and concurrency
primitives need a deliberate profile adaptation; see
[`docs/rust.md`](docs/rust.md#ownership-boundary).

- Keep program-owned counters, IDs, and dispatch state on the root model.
  Mirror external state through construction and events.
- Move cross-thread work over channels; the receiving thread or task owns the
  state. Before proposing a shared lock, name the value and explain why it
  cannot live on the owner or arrive as an event.
- Locks, cells, atomics, `Rc`, and their ecosystem equivalents require the same
  ownership justification. Before using one that `clippy.toml` does not list,
  from `std` or a dependency, add it to `disallowed-types` in the same change,
  then justify the use site with the reasoned expectation.
- Immutable `Arc` snapshots and configuration are permitted. `OnceLock` and
  `LazyLock` are permitted for compute-once immutable constants independent of
  runtime state and configuration. External APIs that require restricted
  primitives still need a reasoned exception at the use site.
- Prefer borrows, moves, and shorter borrows over clones made only to satisfy
  the borrow checker. Deliberate copies of small immutable data and permitted
  snapshot sharing are fine.
- In async code, move blocking work off executor threads and bound CPU work
  between yield points.

### Invariants and errors

Carry invariants in types: newtypes at trust boundaries, enums for variants,
and narrow inputs instead of unreachable match arms. An always-present value
has a non-optional type. A function you own returns `T` rather than
`Result<T, Infallible>`; retain `Infallible` when an external trait requires it.
Use an enum for a parameter that selects a mode or strategy; keep boolean
data, including setter values, as `bool`.

Represent filesystem paths with `Path` and `PathBuf`; convert them to text
explicitly at presentation boundaries. Keep custom equality, ordering, and
hashing consistent, and test their laws. Redact secrets in `Debug` and
`Display`, and test the redaction. Omit `get_` on simple getters. Name `as_`,
`to_`, and `into_` conversions by cost and ownership, as the
[Rust API Guidelines](https://rust-lang.github.io/api-guidelines/naming.html#ad-hoc-conversions-follow-as_-to_-into_-conventions-c-conv)
define them.

Handle external failures with typed errors or explicit failure events.
Discard a `#[must_use]` value only with `let _ =` under a reasoned
`clippy::let_underscore_must_use` expectation; `_ =`, `drop(...)`, an unused
`_name` binding, and an empty error arm are discards the gate cannot see.
Implement `std::error::Error` for custom error types, preserve wrapped causes
through `source()`, and attach the operation and non-sensitive path or value
context. Use iterators or checked access for indexing. Choose `checked_`,
`saturating_`, or `wrapping_` arithmetic according to the domain.

Write program output through fallible I/O, such as `writeln!` on a locked
handle. Propagate or explicitly handle write and flush errors, and return
failures from `main`. Establish intentional ordering at the output boundary;
hash iteration order must not reach output.

Outside test code, never call `assert!`, `assert_eq!`, or `assert_ne!`; a
`# Panics` section documents the panic but does not remove it, and
`tests/allow_policy.rs` rejects the calls. Return a typed error instead,
including where `manual_assert` suggests `assert!`; a justified panic uses
`expect` as described under exceptions. An assertion in a `const` or `static`
initializer or an inline `const` block runs at compile time and is allowed,
unless a closure, function, `async` block, or macro definition there holds it.
Reserve `debug_assert!` for internal contracts whose removal cannot make
release behavior incorrect. Never let a panic cross an FFI boundary.

Put teardown that flushes, sends, syncs, commits, or waits for other work in a
consuming method that returns `Result`, such as `close`, `finish`, or `join`,
and call it on every success path. `Drop` is only the fallback: it never
panics and never waits on another thread, task, process, or peer, so it does
not join, receive from a channel, wait on a lock or child process, call
`block_on`, sleep, retry, or do network I/O, even with a timeout. It may
release what the value owns, including best-effort removal of local files as
`tempfile::TempDir` does; nothing bounds how long that takes.

### Exceptions and workspace policy

`#[allow]` is forbidden. Use one lint in a reasoned
`#[expect(lint, reason = "...")]` on the smallest enclosing statement or item.
Name the invariant that holds and why restructuring would be worse.
Expectations on lint groups or several lints, crate-level expectations
outside test targets, and expectations of a panic lint such as
`clippy::unwrap_used` on a whole module, `impl`, or `trait` outside test code
silence too much;
`tests/allow_policy.rs` rejects them.
Stale expectations fail the lint gate. Lint levels come only from
`[workspace.lints]` and reasoned expectations; `rust:policy` rejects inherited
compiler flags and Cargo configuration that lower them.

For a justified panic assertion, use `expect` with a message naming the
invariant. Clippy's test exemptions cover `#[test]` functions and items under
a `#[cfg(test)]` attribute: they may print, and may unwrap, expect, panic, and
index on invariants they establish. Other test code, such as a top-level
integration-test helper, has no exemption. There, a fallible helper returns
`Result`. No test exemption covers arithmetic, and test code takes no
arithmetic expectation. Prefer literal expected values. Where Clippy reports
test arithmetic, use `checked_` methods, with `expect` naming the invariant
where a test exemption applies and `?` in a `Result`-returning helper
elsewhere. Only a test of an operator implementation itself takes a reasoned
expectation, on the item Clippy reports. Doc examples use `?`, not
`unwrap()`, because Clippy does not lint doctests.

The owner authorized `unsafe_code = "deny"` for the Windows adapters, and
`undocumented_unsafe_blocks = "deny"` is set in `[workspace.lints.clippy]`.
Each unsafe block carries a reasoned expectation and a `SAFETY` comment.
Unsafe code may appear only in the `win.rs` modules of `sources`, `actions`,
and `platform`. A crate root without such a module keeps
`#![forbid(unsafe_code)]`. Widening that scope needs explicit human
authorization.

Every workspace package, including the root package, declares
`[lints] workspace = true`. Release profiles belong in the root manifest;
keep overflow checks enabled. `tests/allow_policy.rs` checks first-party
source inputs, and `rust:policy` checks configured policy probes. Read their
[scope and limitations](docs/rust.md#source-policy-scan) before adding macros,
custom target paths, includes, or symlinks.
Checker-specific production behavior and synthesized suppression attributes
are prohibited even when static analysis cannot detect them.

### Tests and mutation

A behavior change needs a test that fails without it. At trust boundaries,
use `proptest` for invariants and round trips; commit `proptest-regressions/`.
A property must test the contract rather than reimplement the function.
Pin any intermittently killed mutant with a deterministic example test.

A surviving mutant needs a test that kills it, removal of unreachable code,
or a per-site `#[mutants::skip]` with a source reason explaining why no test
can distinguish it. The first skip adds `mutants = "0.0.4"` as a regular
dependency; remove it when the last skip disappears. Classification and
coarser configuration exclusions require human approval. Carry classifications
and their reasons into the handoff.

The gate requires executed mutation evidence: for the code a pull request
changes, and for the whole workspace after it merges.
`.cargo/mutants.toml` excludes the Windows `win.rs` adapters with the owner's
approval, because the Linux gate compiles them out. Read [mutation operation](docs/rust.md#mutation-testing)
before changing report paths, mutation selection, or transaction handling.
Confirm no run is active before clearing a stale lock.

### Dependencies and review

Prefer the standard library. For each added dependency, state the complexity
it removes. `rust:deny` checks dependency policy including development
dependencies; `rust:machete` checks unused regular dependencies. Review
dev/build dependency use separately and report duplicate-version warnings.

For a non-trivial change, obtain an independent read-only review of tests,
changed code, exceptions, and affected contracts. Add focused reviewers when
risk or breadth warrants them. Verify material findings with source evidence
or a regression test; record unresolved design questions explicitly.

Explain enforcement changes and get human approval for relaxations. Report
the checked revision, behavior proved, commands run, and remaining findings.
Recheck affected evidence after further edits. Bots advise, gates block, and
humans merge.

## Git

- Keep generated and local-only files out of commits.
- Follow [Conventional Commits 1.0.0](https://www.conventionalcommits.org/en/v1.0.0/#specification)
  for all git commit messages.
- Write commit messages for future maintainers. Use
  [Tim Pope's note](https://tbaggery.com/2008/04/19/a-note-about-git-commit-messages.html)
  and [Chris Beams' guide](https://cbea.ms/git-commit/) as the Git-specific
  references.
- Keep the Conventional Commit subject concise. Write its description in the
  imperative mood, use the repository's lowercase style, and omit ending
  punctuation. Aim for 50 characters and do not exceed 72 characters.
- Add a body only when it provides useful context. Separate it from the subject
  with a blank line, wrap it at 72 characters, and explain the problem, the
  reason for the change, and material consequences. Let the diff explain the
  implementation.
- Use short, direct sentences, active voice, and consistent terminology. Avoid
  idioms, filler, vague wording, and unnecessary jargon.
- One writer owns one worktree, and reviewers stay read-only. Serialize
  lockfiles, shared configuration, formatters, and integration. Give each
  worktree its own build directory.

## Handoff

- Get an independent read-only agent review of the actual diff for
  non-trivial behavior and architecture changes. The author does not approve
  their own change. Verify material findings before acting on them.
- Report to whoever asked for the change, not in the PR description: the
  checked revision, behavior proved, commands run and their
  results, skipped checks and why, remaining findings, and tree growth the
  change does not explain. Write it so the human can judge the outcome without
  reconstructing the implementation.
- Bots advise, gates block, and humans merge.
