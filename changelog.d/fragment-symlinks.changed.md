- A symlink in the fragment directory is `not-a-fragment` and is never
  read, for changelog and TODO fragments alike. 0.1.0 followed a symlink
  under `changelog.d/` to the file it named.
