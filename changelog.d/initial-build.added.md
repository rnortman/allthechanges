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
