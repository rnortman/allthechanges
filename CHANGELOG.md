# Changelog

All notable changes to this project are documented in this file. The
format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and the project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-10-23

### Added

- TODO fragments: `TODO.md` entries can now be written one per file, as
  `todo.d/<slug>.md` (or `.todo.d/`), so parallel branches stop colliding
  in `TODO.md` the way `changelog.d/` already stops them colliding in
  `CHANGELOG.md`. `allthechanges todo query` lints and lists the pending
  fragments; `allthechanges todo gather` inserts each into the `## Open`
  section of `TODO.md` as `### <slug>`, in alphabetical order, and deletes
  it. Two new problem codes: `not-an-entry` (a body with a heading outside a
  code fence, an unclosed fence, or a blank first line) and `present`
  (`TODO.md` already has an entry with that slug).

### Changed

- A symlink in the fragment directory is `not-a-fragment` and is never
  read, for changelog and TODO fragments alike. 0.1.0 followed a symlink
  under `changelog.d/` to the file it named.

## [0.1.0] - 2026-10-23

### Added

- A Bazel-only build: the Rust, LLVM and musl toolchains and every tool the
  gate runs are pinned and fetched by checksum, so a clone needs only
  bazelisk, git and make. `make check` is the single gate (build, tests,
  clippy, rustfmt, buildifier, secret scan).
- Static musl release binaries for Linux x86_64 and aarch64 (`make
  release`), attached to the GitHub release by a workflow that runs on
  every `v*` tag.
- Secret scanning with brenn-scrub, built through Bazel, wired into the
  pre-commit and pre-push git hooks (`make setup`) and Claude Code write
  hooks.
- GitHub Actions CI running `make check` and the musl release build on
  every push and pull request, and requiring every pull request to add a
  changelog fragment unless it carries the `skip-changelog` label.
- `allthechanges`, a changelog-fragment tool: each change's entry is its own
  file under `changelog.d/` (or `.changelog.d/`), named
  `[<YYYY-MM-DD>.]<slug>.<section>.md` and holding Markdown list items, so
  parallel branches never conflict on `CHANGELOG.md`.
- `allthechanges query` lints and lists the fragments; with `--range` it
  lists only those added in a git range, and `--require` fails when there is
  none, for a CI check that every pull request adds an entry.
- `allthechanges gather --version <v>` moves every fragment into a new
  `## [<v>] - <date>` section of `CHANGELOG.md` and deletes the fragment
  files, without running git. `--dry-run` prints the section instead.
- Every command has a `--json` report and fixed exit codes: 0 done, 1 the
  check failed, 2 usage error, 3 operational error. stdout carries the
  report and nothing else; stderr carries diagnostics only.
