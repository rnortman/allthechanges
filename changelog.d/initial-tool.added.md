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
