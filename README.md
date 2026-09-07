# Comic Name

Comic Name is a GNOME app for explicitly matching comic archives and PDFs to
ComicVine issues, reviewing the resulting names, and renaming consistently:

`Series (Volume Year) Annual #Issue (Cover Month Cover Year).cbz`

For example: `Batman (2014) #1 (October 2014).cbz`.

Comic Name never chooses a series or issue automatically. Add your ComicVine
API key in Preferences, then choose one of two workflows:

- Open one comic, search for its series, choose one issue, and confirm its name.
- Open a folder, choose one series, and align its fixed issue list with the
  local files by moving rows or creating gaps before confirming the batch.

Renames use portal-aware GIO file moves and never overwrite existing files.

## Development

The host does not provide a C compiler, so run Rust tests and Clippy with the
GNOME 50 Flatpak SDK:

```bash
flatpak run --command=sh \
  --filesystem="$PWD" \
  --filesystem="$HOME/.cargo" \
  --filesystem="$HOME/.rustup:ro" \
  --socket=wayland \
  --socket=fallback-x11 \
  --env=PATH="$HOME/.cargo/bin:/usr/bin" \
  org.gnome.Sdk//50 -c 'cargo test --offline'
```

```bash
cargo fmt --check
```

```bash
flatpak run --command=sh \
  --filesystem="$PWD" \
  --filesystem="$HOME/.cargo" \
  --filesystem="$HOME/.rustup:ro" \
  --env=PATH="$HOME/.cargo/bin:/usr/bin" \
  org.gnome.Sdk//50 -c 'cargo clippy --all-targets --offline -- -D warnings'
```
