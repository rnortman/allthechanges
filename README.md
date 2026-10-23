# allthechanges

Changelog fragments for [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
files. Every change carries its changelog entry as a file of its own under
`changelog.d/`, instead of an edit to the top of `CHANGELOG.md`, so branches
developed in parallel never conflict on the changelog. At release time
`allthechanges gather` folds the fragments into a new release section of
`CHANGELOG.md` and deletes them.

Two commands:

- `allthechanges query` lints and lists the fragments; with `--range` it
  lists only those added in a git range, which is the CI check "this pull
  request adds a changelog entry".
- `allthechanges gather --version X.Y.Z` writes the release section and
  deletes the fragments. It runs no git: a person or a release bot commits
  the result.

Both have a human report and a `--json` one, fixed exit codes, and a fixed
split between stdout and stderr, so scripts and LLM agents can drive them
without parsing prose.

## Install

```sh
cargo install allthechanges
```

or download a static binary for `x86_64-unknown-linux-musl` or
`aarch64-unknown-linux-musl` from the
[GitHub releases](https://github.com/rnortman/allthechanges/releases), each
with a `SHA256SUMS` file.

## Fragments

A fragment is a file in `changelog.d/` (or `.changelog.d/`; a repository
with both is an error) named

```
[<YYYY-MM-DD>.]<slug>.<section>.md
```

- `slug` is letters, digits, `-` and `_`; it only has to be unique.
- `section` is one of the Keep a Changelog sections, lowercase: `added`,
  `changed`, `deprecated`, `removed`, `fixed`, `security`.
- The optional date orders fragments within a section: dated fragments
  first, by date, then undated ones, each by slug.

The body is one or more Markdown list items, copied into the changelog
verbatim. The first line starts with `- `; every other line is blank,
another `- ` item, or indented by at least two spaces:

```markdown
- `export --csv` writes the report as CSV. Columns follow the order of
  the on-screen table.
```

```sh
mkdir -p changelog.d
printf -- '- Fix the login timeout when the server is slow.\n' > changelog.d/login-timeout.fixed.md
```

Anything else is a lint problem, never rewritten: prose without a `- `,
`* ` bullets, headings, an empty file, a name that does not parse. Files
whose names begin with `.` are ignored, so `changelog.d/.gitkeep` keeps an
otherwise empty directory in git. A missing `changelog.d` is not an error:
it holds no fragments.

## `query`

```
allthechanges [--dir <path>] [--json] query [--range <git-range>] [--require]
```

Lists every fragment, grouped by section in changelog order, then the
problems:

```
$ allthechanges query
changelog.d: 2 fragments, 1 problem

### Added

- `export --csv` writes the report as CSV.  [2026-10-20.csv-export.added.md]

### Fixed

- Fix the login timeout when the server is slow.  [login-timeout.fixed.md]

Problems:
changelog.d/typo.fxied.md: unknown-section: "fxied" is not a section; expected one of added, changed, deprecated, removed, fixed, security
```

- `--range <git-range>` lists only the fragments `git diff` reports as
  added in the range (`A..B`, `A...B` or a single ref), read from the
  working tree. A fragment git reports that is no longer on disk is the
  problem `missing`. Needs `git` on `PATH`, and asks it even when the
  fragment directory does not exist, so a bad range is always an error.
- `--require` fails unless at least one fragment is listed.
- `--dir <path>` names the fragment directory, relative to the current
  directory; by default it is `changelog.d`, else `.changelog.d`.

## `gather`

```
allthechanges [--dir <path>] [--json] gather --version <v> [--date <YYYY-MM-DD>] [--changelog <path>] [--dry-run]
```

Lints first and writes nothing if there is any problem, if there are no
fragments, or if `CHANGELOG.md` already has a section for the version.
Otherwise it writes

```markdown
## [1.4.0] - 2026-10-23

### Added

- `export --csv` writes the report as CSV.

### Fixed

- Fix the login timeout when the server is slow.
```

after the `## [Unreleased]` section if there is one, else before the first
release, else at the end of the file, and then deletes the fragments it
gathered. Nothing else in `CHANGELOG.md` changes: an `[Unreleased]` body,
earlier releases and link-reference definitions stay as they are. A missing
`CHANGELOG.md` is created with the standard Keep a Changelog header.

- `--version` is written verbatim (`v1.4.0` stays `v1.4.0`); no whitespace,
  `[` or `]`.
- `--date` defaults to today in local time.
- `--changelog` defaults to `CHANGELOG.md` in the current directory. It
  must be a regular file or absent; a symlink is an error. The new text is
  written to `<changelog>.tmp` and renamed into place, and anything already
  at that temporary path is an error, left untouched.
- `--dry-run` prints the section and writes and deletes nothing.

## Exit codes and streams

| Code | Meaning |
|---|---|
| 0 | Done; for `query`, no problems and (with `--require`) at least one fragment. |
| 1 | The check failed: lint problems, `--require` with no fragment, or `gather` refused. |
| 2 | Usage error. |
| 3 | Operational error: an unreadable file, git failed or absent, not a git repository, both `changelog.d` and `.changelog.d` present, `CHANGELOG.md` not UTF-8 or with CRLF line endings. |

stdout carries the report and nothing else; stderr carries diagnostics
only. On exit 0 and 1 stderr is empty and stdout holds the report; on exit
2 and 3 stdout is empty and stderr says what went wrong (exit 3 as one line,
`allthechanges: <message>`).

## JSON

`--json` (before or after the subcommand) prints exactly one JSON document
and a newline.

`query`:

```json
{
  "dir": "changelog.d",
  "range": null,
  "fragments": [
    {
      "path": "changelog.d/login-timeout.fixed.md",
      "date": null,
      "slug": "login-timeout",
      "section": "fixed",
      "body": ["- Fix the login timeout when the server is slow."]
    }
  ],
  "problems": [
    {
      "path": "changelog.d/typo.fxied.md",
      "code": "unknown-section",
      "message": "\"fxied\" is not a section; expected one of added, changed, deprecated, removed, fixed, security"
    }
  ],
  "ok": false
}
```

`dir` is `null` when neither default directory exists. `ok` is true when
there are no problems and, under `--require`, at least one fragment. The
problem codes are `bad-name`, `unknown-section`, `not-a-fragment`,
`not-utf8`, `empty`, `not-a-list` and `missing`.

`gather`:

```json
{
  "changelog": "CHANGELOG.md",
  "created": false,
  "version": "1.4.0",
  "date": "2026-10-23",
  "dry_run": false,
  "gathered": ["changelog.d/2026-10-20.csv-export.added.md", "changelog.d/login-timeout.fixed.md"],
  "rendered": "## [1.4.0] - 2026-10-23\n\n### Added\n\n- `export --csv` writes the report as CSV.\n\n### Fixed\n\n- Fix the login timeout when the server is slow.\n",
  "problems": [],
  "refused": null,
  "ok": true
}
```

`rendered` is the inserted section, ready to reuse as release notes.
`refused` is `null` or why nothing was written: `problems` (listed in
`problems`), `nothing-to-gather`, or `version-present` — the last is what a
release bot sees when it re-runs after the release was already gathered.

## In CI: every pull request adds a fragment

```yaml
on:
  pull_request:
    # labeled and unlabeled re-run the check when the label changes.
    types: [opened, synchronize, reopened, labeled, unlabeled]

jobs:
  changelog:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7
        with:
          fetch-depth: 0
      # ... install allthechanges ...
      - if: ${{ !contains(github.event.pull_request.labels.*.name, 'skip-changelog') }}
        env:
          BASE: ${{ github.base_ref }}
        run: allthechanges query --range "origin/$BASE...HEAD" --require
```

The `skip-changelog` label is the escape hatch, and it belongs in the
workflow rather than the tool. A release pull request deletes fragments and
adds none, and a docs-only or CI-only change has nothing to tell a user; a
required check that the release it exists to produce cannot satisfy is a
check that gets bypassed. A label on the pull request is visible and
reviewable.

## Building and contributing

Install [bazelisk](https://github.com/bazelbuild/bazelisk) as `bazel`; it
reads `.bazelversion` and fetches the pinned Bazel. Every toolchain is
pinned in `MODULE.bazel` and fetched by checksum, so nothing else is needed
on the host but git and make.

```sh
make setup                   # once per clone: wires the git hooks
make check                   # the full gate, exactly what CI runs
make run ARGS="query"        # build and run allthechanges in this directory
make release                 # the static musl binaries for x86_64 and aarch64
make help                    # every target
```

The `Makefile` is a frontend: every target is a spelled-out `bazel`
invocation. Every change adds a fragment to this repository's own
`changelog.d/`.

## Releasing

On a branch:

```sh
make run ARGS="gather --version X.Y.Z"
```

Set `version` in `Cargo.toml` and `VERSION` in `BUILD.bazel` to `X.Y.Z`,
then run `make repin`. Commit the gathered `CHANGELOG.md`, the deleted
fragments, both manifests and every lock file `make repin` changed —
`Cargo.lock` and `cargo-bazel-lock.json` always change with the package
version, and `make publish` refuses a stale `Cargo.lock`. Open a pull
request labelled `skip-changelog` and merge it. Tag the resulting `main`
commit and push the tag:

```sh
git tag vX.Y.Z
git push origin vX.Y.Z
```

The release workflow checks the tag against `Cargo.toml`, runs the gate,
and attaches both binaries, their `SHA256SUMS` and the release's changelog
section to a GitHub release. Then publish the crate:

```sh
CARGO_REGISTRY_TOKEN=... make publish
```

## License

Apache-2.0; see `LICENSE` and `NOTICE`.
