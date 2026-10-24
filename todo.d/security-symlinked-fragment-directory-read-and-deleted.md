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
