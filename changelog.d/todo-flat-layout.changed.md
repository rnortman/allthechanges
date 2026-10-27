- **`TODO.md` uses a flat layout; a 0.2.0-style file must be migrated.**
  `todo gather` now writes each entry as `` ## `<slug>` `` directly under
  the file's `# TODO` title, in alphabetical order, instead of as
  `### <slug>` inside a `## Open` section. A newly created `TODO.md` is just
  `# TODO`. To migrate a file written by 0.2.0: delete the `## Open` line,
  turn each `### <slug>` into `` ## `<slug>` ``, and delete any other
  section (such as `## Done`) together with its entries — left in place,
  `## Done` would be read as an entry named `Done` and everything under it
  as open entries.
- `todo gather` refuses a `TODO.md` it cannot read safely — one still in
  the old layout (a `## Open` section or a `### ` heading), with a `## `
  heading that is not a slug, a `# ` heading after the first entry, or a
  code fence that is never closed. It exits 3 naming the offending line and
  writes nothing.
- `todo query`'s human-readable report shows each fragment under
  `` ## `<slug>` `` to match.
