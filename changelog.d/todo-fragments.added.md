- TODO fragments: `TODO.md` entries can now be written one per file, as
  `todo.d/<slug>.md` (or `.todo.d/`), so parallel branches stop colliding
  in `TODO.md` the way `changelog.d/` already stops them colliding in
  `CHANGELOG.md`. `allthechanges todo query` lints and lists the pending
  fragments; `allthechanges todo gather` inserts each into the `## Open`
  section of `TODO.md` as `### <slug>`, in alphabetical order, and deletes
  it. Two new problem codes: `not-an-entry` (a body with a heading outside a
  code fence, an unclosed fence, or a blank first line) and `present`
  (`TODO.md` already has an entry with that slug).
