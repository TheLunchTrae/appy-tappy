# Rust standards

This page explains how the Rust checks work and where they stop.
[`AGENTS.md`](https://github.com/TheLunchTrae/appy-tappy/blob/main/AGENTS.md)
holds the rules; `Cargo.toml`, `clippy.toml`, `deny.toml`, and
`.config/mise/conf.d/rust.toml` hold the enforcement.

## Toolchain

`rust-toolchain.toml` is the one compiler pin. mise reads it, so `mise install`,
rustup, and editors select the same compiler and components. `rust-version` in
`Cargo.toml` is the minimum supported Rust version that Clippy reads; raise it
deliberately.

mise installs cargo-deny from aqua, and cargo-machete, cargo-nextest, and
cargo-mutants from their GitHub release binaries. cargo-mutants publishes
x86_64 binaries only, so mise skips it on macOS.

Cargo tasks share one build-directory lock, so the gate runs Clippy, the
documentation build, the tests, and the package check in a fixed order.
`rust:policy` and cargo-mutants run beside them; cargo-mutants builds in its
own copy of the tree.

## Lint wall

The workspace pins Rust, uses edition 2024, denies `unsafe` code outside the
Windows adapters (see [architecture](Architecture.md#unsafe-code)), requires
documented public API with `Debug` implementations, denies rustdoc warnings,
checks doctests, and runs Clippy on every target and feature with warnings
promoted to errors. The wall lives in `[workspace.lints]`, and every package
inherits it with `[lints] workspace = true`. Comments in `Cargo.toml` name the
invariant each group defends and the reason for each relaxed lint.

Panic-class lints, the print macros, and the shared-state types that
`clippy.toml` disallows are denied. Test code gets only the exemptions that the
`allow-*-in-tests` settings in `clippy.toml` grant. Release builds keep integer
overflow checks.

An exception is a per-site `#[expect(lint, reason = "...")]`, which fails once
the lint stops firing. `#[allow]` is rejected everywhere.

## Ownership boundary

`clippy.toml` bans `Mutex`, `RwLock`, `Rc`, atomics, and similar types. That
encodes one architecture: a single owner for application state, pure
`(state, event)` transitions, and messages to the owner. A library or driver
that needs shared state, or a framework that expects it, such as Tauri's
managed state, changes `disallowed-types`, the ownership rules in `AGENTS.md`, and the
state cases in `policy-fixtures/lint-wall.rs.in` together, with the owner's
approval. A single site inside an application uses a reasoned `#[expect]`.

## Source policy scan

`tests/allow_policy.rs` checks that every workspace package, the root included,
inherits the lint wall. It parses every first-party Rust source and rejects:

- `#[allow]` in any form, including `cfg_attr`-nested, raw `r#allow`, and
  attributes inside macro bodies;
- `#[expect]` that names more than one lint, a lint group, or an
  allow-attribute lint;
- crate-level `#![expect]` outside test roots;
- outside test code, panic-lint expectations that cover a module, `impl`, or
  `trait`, and calls to `assert!`, `assert_eq!`, and `assert_ne!`.

Test code is a Cargo test target, an item under a `cfg` that requires `test`,
or a function with a `test` attribute. The scan follows the module tree from
each target root through `mod` declarations and literal `include!` inputs, and
follows in-project symlinks.

The scan cannot see macro output. A macro that builds an `allow` from split or
generated tokens is a prohibited bypass that review must catch. Non-literal
`include!` and custom `#[path]` modules fail, because the scan cannot prove
their inputs. Every input must stay inside the project root. The scan compares
POSIX paths, so it runs on Linux or macOS, not Windows.

## Lint-wall proof

`rust:policy` proves that the wall fires. `tests/lint_wall.rs` replays the
`rust:lint` command over `policy-fixtures/lint-wall.rs.in` in a scratch package
that takes the workspace's lints, edition, and `rust-version`. Each
`//~ ERROR <code>` comment marks a diagnostic that line must report; a missing
or unexpected diagnostic fails the task. The proof also fails on these routes
that could lower lint levels without the probe noticing: extra keys or
arguments on `rust:lint`; `-A`, `--allow`, `--force-warn`, `--cap-lints`,
unstable `-Z`, or `@file` flags in the Clippy arguments, `RUSTFLAGS`-style
variables, or Cargo config `rustflags`; a forced `CLIPPY_ARGS`; or a
`CLIPPY_CONF_DIR` outside the project. Change the probe and the test's list of codes together with the
wall.

## Dependencies

`rust:lock` refreshes `Cargo.lock` without upgrading unrelated packages. Every
other Cargo task runs with `--locked`, so a stale lock fails. `rust:deny` fails
on advisories, unmaintained or yanked crates, disallowed licenses, wildcard
requirements, and unknown registries, and warns on duplicate versions.
`rust:machete` fails on unused `[dependencies]` entries; it does not check dev
or build dependencies.

## Mutation testing

`rust:mutants` is the full sweep. `rust:mutants:diff` mutates code changed
since the merge base with `MUTANTS_BASE_REF`, which defaults to `origin/main`,
then local `main`, so it needs full Git history. Untracked files fail the
lane; stage them with `git add -N`. A diff that selects no mutants passes.

`rust:standards:check` runs `rust:mutants:gate`, which picks the lane from
`MUTANTS_LANE`: `full`, the default, or `diff`. CI runs the `diff` lane for
pull requests and merge groups, against the base that
`.github/workflows/quality.yml` sets, so the quality job fits its time limit.
Pushes to main and manual runs take the full sweep, split into shards across
the `quality` and `mutants` jobs:
`MUTANTS_SHARD=k/n` makes `rust:mutants` run shard `k` of `n`, counted from
zero, and `.github/workflows/quality.yml` sets the shard count.

The diff lane mutates only changed source lines, so a pull request that only
removes or weakens tests, or changes only mutation configuration, selects no
mutants. Run the workflow manually on its branch to check the whole workspace
before merging it.

`rust:mutants` requires at least one mutant to run. Both lanes clear and rewrite
`mutants.out/` and validate its JSON, so stale evidence cannot pass. A report in
which every mutant is unviable fails. A surviving mutant is a review finding.

Both lanes hold `target/standards-mutants.lock` through the run and its
verification, so a second run fails instead of replacing the report. If a
crash leaves the lock behind, the error names the directory. Confirm that no
mutation task is running, then remove it. The lock uses `setsid` and
process-group signals, so mutation runs on Linux only.

## Repository settings

`CODEOWNERS` assigns every path to the owner, because source files can carry
mutation classifications and lint expectations. The settings that make that a
gate live on GitHub: require the `quality` check and Code Owner review,
dismiss stale approvals on new commits, and disallow bypassing protection.
