# allthechanges — agent instructions

## What this is

`allthechanges` keeps `CHANGELOG.md` free of merge conflicts: each change's
entry is a fragment file under `changelog.d/`, `query` lints and lists them,
and `gather` folds them into a release section of `CHANGELOG.md`. `todo
query` and `todo gather` do the same for `TODO.md` entries under `todo.d/`.
Usage,
format and contract: `README.md`. Read it first.

## Build: Bazel, and only Bazel

Bazel is the build. The Makefile is a frontend whose every target is a
spelled-out `bazel` invocation; there is no target that runs `cargo`, and the
gate never will.

- Every toolchain and every tool the gate runs is pinned in `MODULE.bazel` and
  fetched by checksum. The only host prerequisites are bazelisk (as `bazel`),
  git and make.
- Nothing in a build or test action may reach for a host-installed program.
  A test that needs a service, a fixture generator or a linter gets it as a
  Bazel dependency. If a tool is genuinely impractical to bring in through
  Bazel, pick a different tool, and if that is not possible either, surface it
  as a decision rather than adding a host dependency.
- `Cargo.toml` + `Cargo.lock` remain the source of truth for crate versions
  and features; `cargo-bazel-lock.json` is their rendered view. A second crate
  hub under `tools/scrub/` pins the secret scanner the same way, with its own
  `Cargo.lock` and `cargo-bazel-lock.json`.
- `make repin` re-renders both hubs at once. After any manifest change or
  scanner bump, run it and commit every lock file that changed.
- `RUST_VERSION` in `MODULE.bazel`, the channel in `rust-toolchain.toml`,
  `rust-version` in `Cargo.toml` and `msrv` in `clippy.toml` are held equal
  by `//tools/pins:pins_test`;
  so are the package version in `Cargo.toml` and `VERSION` in the root
  `BUILD.bazel`. `rust-toolchain.toml` only serves the tools that
  run outside Bazel (rust-analyzer).

## Commands

| Command | What it does |
|---|---|
| `make check` | The gate: scrub kit check, build, every test, clippy, rustfmt, buildifier, tree scan |
| `make build` | Build every target |
| `make test` | Every test, without the lint aspects (the inner loop) |
| `make run ARGS=...` | Build and run the `allthechanges` binary in the current directory |
| `make release` | Build the static musl release binaries for x86_64 and aarch64 |
| `make publish` | `cargo publish --locked` with the pinned toolchain's cargo; the owner's step, with `CARGO_REGISTRY_TOKEN` set |
| `make fix` | rustfmt and buildifier the tree in place |
| `make repin` | Re-render every crate hub's lock files |
| `make ide` | Write `rust-project.json` for rust-analyzer |
| `make setup` | Wire git at `.githooks` (once per clone) |
| `make scrub-tree` | Whole-tree secret sweep |
| `make clean` | Drop Bazel's outputs for this workspace |

## Gates

- A commit scans the staged change for secrets and then runs `make check`.
- A push scans the range being pushed.
- CI runs `make check` on every push and pull request, with the module
  lockfile in error mode, so a resolution nobody committed is a red build.
- The Claude Code write hook scrubs every write before it lands and blocks on
  any failure, including a scanner that fails to start.

Every scan runs `bazel run //tools/scrub:brenn-scrub`. Treat the local hooks
as the gate and CI as a backstop, not the other way round: a CI run says
nothing about the commit sitting in your working tree.

If a gate blocks a write or a commit, **surface it**. Never route around it.
The gate being wrong is a thing that happens and is worth reporting; a
bypassed gate is not recoverable once it has been pushed.

## Principles

- Fail fast. Invalid input is rejected, not coerced: a fragment that is not
  a Markdown list is a lint problem, never rewritten into one. No fallbacks
  that hide a missing authority.
- No over-engineering, no premature abstraction, no hypothetical features.
  Scope discipline: do what was asked.
- The stream and exit-code contract below is an interface.

## Interface

The tool's observable behaviour is its interface, and changing any of it is a
breaking change that goes in a `changed` fragment:

- Exit codes: 0 done, 1 the check failed (lint problems, `--require` with no
  fragment, `gather` or `todo gather` refused), 2 usage error, 3 operational error.
- Streams: stdout carries the report and nothing else (with `--json`, exactly
  one JSON document and a newline); stderr carries diagnostics only. Nothing
  reaches stderr on exit 0 or 1; stdout is empty on exit 2 and 3.
- The JSON field names, the problem codes (`bad-name`, `unknown-section`,
  `not-a-fragment`, `not-utf8`, `empty`, `not-a-list`, `not-an-entry`,
  `present`, `missing`) and the
  `refused` values (`problems`, `nothing-to-gather`, `version-present`).

`gather` touches only the section it inserts: the `[Unreleased]` body,
earlier releases and link-reference definitions are never rewritten.
`todo gather` touches only the entries it inserts.

## The one host program

Tests of `query --range` spawn the `git` on `PATH` from inside the sandbox,
through `brenn-git-fixture`, which strips `GIT_*`, pins config to `/dev/null`
and checks the fixture never escapes its tempdir. It is the single host
program a build or test action reaches: git is already a host prerequisite
for the hooks, no checksummed git release archive exists for Bazel to pin,
and the tool's only git dependency is `git diff`. No other test may follow
this precedent without a decision.

## Changelog

Every change adds a fragment to `changelog.d/` (`<slug>.<section>.md`, a
Markdown list). CI fails a pull request without one unless it carries the
`skip-changelog` label, which is for release pull requests and changes with
nothing to tell a user. Never edit `CHANGELOG.md` by hand;
`make run ARGS="gather --version X.Y.Z"` writes it at release time.

## Releasing

On a branch: `make run ARGS="gather --version X.Y.Z"` and
`make run ARGS="todo gather"`; set `version` in
`Cargo.toml` and `VERSION` in `BUILD.bazel` to `X.Y.Z`; `make repin` (the
root package's entry in `Cargo.lock` and the crate_universe lock change with
the version); commit the gathered changelog and `TODO.md`, the fragments
deleted from `changelog.d/` and `todo.d/`, both
manifests and every lock file that changed. Open a pull request labelled
`skip-changelog` and merge it, tag the resulting `main` commit `vX.Y.Z` and
push the tag (the release workflow attaches the musl binaries), then
`make publish` with `CARGO_REGISTRY_TOKEN` set.

## Code standards

- Rust edition 2024, the pinned stable toolchain. The edition is set once
  for Bazel, on the toolchain in `MODULE.bazel`; BUILD targets do not
  restate it.
- `make check` must pass: clippy with warnings denied, rustfmt, buildifier,
  every test.
- No unused code. No backwards-compatibility shims.
- Default branch: `main`.

## TODO system

Two pieces that stay in sync:

- `TODO.md` at the repo root: the master list. Each entry has a slug, a
  description, and the deferral context.
- `TODO(slug)` comments in code, marking the spot where the work happens.

A new entry is written as a fragment, `todo.d/<slug>.md` holding the
description and context — never as an edit of `TODO.md`.
`make run ARGS="todo gather"` folds the fragments into `TODO.md` at release
time, so between releases the open list is `TODO.md` plus `todo.d/`;
`make run ARGS="todo query"` lists the pending half. Removing a done TODO
deletes its fragment if it still is one, else its `TODO.md` entry, and its
comment.

Slugs are the join key; adding a TODO requires both halves. Don't use TODOs for
vague aspirations. Every TODO describes a concrete thing, in a place where
"done" is obvious.

## Public tree

Everything tracked here is published. `TODO.md`, `CHANGELOG.md`, commit
messages and every code comment are public writing: no internal host names,
no absolute paths into other trees, no references to documents a public reader
cannot follow. Design documents live outside this repository and are never
cited from code.
