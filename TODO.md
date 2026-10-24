# TODO

The master list of deferred work. Each entry has a slug, a description, and the
reason it was deferred; the matching `TODO(slug)` comment in code marks the spot
where the work happens. Slugs are the join key: adding a TODO requires both
halves, and removing one removes both.

Every entry is public writing.

## Open

### dependency-audit-lane

A license and advisory audit of the Rust dependency graph (for example
`cargo deny`) belongs in `make check` before the first crate is published.
Running it through Bazel needs a decision first: the advisory check fetches
the RustSec database over the network, which a sandboxed test action must not
do, and the tool shells out to `cargo metadata`, so it needs the pinned
toolchain's cargo handed to it. The options are a repository rule that
fetches the advisory database at a pinned commit so the check is a sandboxed
test; an unsandboxed `bazel run` lane like the scrub gate; or a licenses-and-
bans test now with advisories later. The `TODO(dependency-audit-lane)` comment
sits above `check` in the `Makefile`. The repository has one crate hub for
the package and one under `tools/scrub` for the scanner; both are in scope.

### security-symlinked-fragment-directory-read-and-deleted

`locate` in `src/scan.rs` follows a `changelog.d`, `.changelog.d`, `todo.d`
or `.todo.d` that is itself a symlink. Only the entries inside are checked
for being symlinks, so a branch that commits `todo.d -> <path>` makes
`todo query` print every `<slug>.md` file in `<path>`, and `todo gather`
copy them into `TODO.md` and then delete them from `<path>`; the changelog
kind does the same for list-shaped `<slug>.<section>.md` files. The likely
fix is the stance `target::read` takes for a symlinked `CHANGELOG.md` or
`TODO.md`: a default fragment directory that is a symlink is an error
(exit 3). It needs a decision first: whether an explicit `--dir` naming a
symlink is still followed, whether parent components of the path are in
or out, and acceptance of a breaking change, since 0.1.0 follows a
symlinked `changelog.d`. The `TODO(security-symlinked-fragment-directory-read-and-deleted)`
comment sits above `locate`.
