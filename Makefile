# Repo-root Makefile: a frontend to Bazel, the check gate, and hook wiring.
#
# Bazel is the build. Every recipe here is a spelled-out Bazel invocation (or
# git plumbing), so `check` is one definition that the pre-commit hook and CI
# both run, reproducible on a fresh clone with nothing installed but bazelisk.

# Recipes run under bash with -e and pipefail, so a failing command anywhere in a
# chain or a pipeline fails the target. Without this a broken step degrades into a
# green result, which is the worst thing a gate can do.
SHELL := /bin/bash
.SHELLFLAGS := -eu -o pipefail -c

.DEFAULT_GOAL := help

SCRUB := bazel run //tools/scrub:brenn-scrub --

.PHONY: help
help:
	@echo "Repo-root targets (all of them run Bazel):"
	@echo "  make check         the gate: scrub kit, build, test, clippy, rustfmt, buildifier, tree scan"
	@echo "  make build         build every target"
	@echo "  make test          run every test, without the lint aspects"
	@echo "  make run           run the allthechanges binary in this directory (ARGS=... for arguments)"
	@echo "  make release       build the static musl release binaries for x86_64 and aarch64"
	@echo "  make publish       cargo publish --locked with the pinned cargo (CARGO_REGISTRY_TOKEN set)"
	@echo "  make fix           rustfmt and buildifier the tree in place"
	@echo "  make repin         re-render every crate hub's lock files after a Cargo.toml change or scrub bump"
	@echo "  make ide           write rust-project.json for rust-analyzer"
	@echo "  make setup         wire git at .githooks (once per clone)"
	@echo "  make scrub-tree    whole-tree secret sweep"
	@echo "  make clean         drop Bazel's outputs for this workspace"

# The gate, in three steps. The scrub kit check is cheap and its failure the
# most important, so it runs first. One `bazel test` then carries the build of
# every target, every test, and (via the config) the clippy and rustfmt
# aspects over every Rust target. The tree scan runs last: it is the backstop
# that sees the whole tracked tree. Every tool is pinned in MODULE.bazel and
# fetched by checksum, so no lane can skip.
# TODO(dependency-audit-lane): add a license and advisory audit of the Rust
# dependency graph, run through Bazel.
.PHONY: check
check:
	$(SCRUB) init --check
	bazel test --config=lint //...
	$(SCRUB) tree

.PHONY: build
build:
	bazel build //...

.PHONY: test
test:
	bazel test //...

# `bazel run` starts the binary in its runfiles tree; --run_under puts it back
# in the directory make was invoked from, where changelog.d, CHANGELOG.md,
# todo.d and TODO.md are.
ARGS ?=
.PHONY: run
run:
	bazel run --run_under="cd $$PWD &&" //:allthechanges -- $(ARGS)

# The two static musl binaries, optimised. Each target transitions
# //:allthechanges to its platform itself, so no --platforms is passed.
.PHONY: release
release:
	bazel build --config=release //tools/release:allthechanges-x86_64-unknown-linux-musl //tools/release:allthechanges-aarch64-unknown-linux-musl

# The owner's publish step, with CARGO_REGISTRY_TOKEN set. The one recipe that
# runs cargo: rules_rust's wrapper around the pinned toolchain's cargo, run in
# this directory. It is a publish, not a build, and not part of `check`; its
# verification build links with the host cc.
.PHONY: publish
publish:
	bazel run --run_under="cd $$PWD &&" @rules_rust//tools/upstream_wrapper:cargo -- publish --locked

# Scoped to what `check` enforces that has a fixer, so `make fix && make check`
# is a clean cycle. Clippy findings are fixed by hand.
.PHONY: fix
fix:
	bazel run @rules_rust//:rustfmt
	bazel run //:buildifier

# crate_universe renders each Cargo.lock into a cargo-bazel-lock.json once and
# then trusts it. CARGO_BAZEL_REPIN re-resolves every hub the crate extension
# declares, the workspace's and tools/scrub's, with the pinned cargo; a hub
# whose inputs did not change renders byte-identically. Commit every lock file
# that changed.
.PHONY: repin
repin:
	CARGO_BAZEL_REPIN=1 bazel fetch --repo=@crates --repo=@scrub_crates

# rust-analyzer's view of the build. Not part of `check`: it writes into the
# source tree, and the file it writes is ignored.
.PHONY: ide
ide:
	bazel run @rules_rust//tools/rust_analyzer:gen_rust_project

# Wire git at the tracked hooks directory through the scrub kit's own init,
# which sets core.hooksPath, removes a stale .git/hooks/pre-commit and lays
# nothing that is already present. Idempotent; run once per clone.
.PHONY: setup
setup:
	@command -v bazel >/dev/null 2>&1 || { \
	    echo "bazel not found on PATH: install bazelisk as 'bazel'; every gate runs through it." >&2; \
	    exit 1; }
	$(SCRUB) init

.PHONY: scrub-tree
scrub-tree:
	$(SCRUB) tree

.PHONY: clean
clean:
	bazel clean
