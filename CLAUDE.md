# Patina

Patina is a dual-pane file manager for macOS, with a Rust/Tauri backend and a Svelte frontend. The README covers installation and development.

## Releases

Every push to main is a source release. Commit the changes on main and pick the appropriate version increment.

```bash
npm run release -- patch   # fixes
npm run release -- minor   # new features
```

The release command updates the version, runs the checks, and builds a local app with ad hoc signing. It pushes the release tag and publishes a GitHub release with source archives. Release notes come from commit messages.

The same command publishes the Homebrew formula to DistinctZA/homebrew-tap with the SHA-256 of the tagged source archive. Edit `packaging/patina.rb.in` to change how Homebrew builds Patina. Publishing requires Git and GitHub CLI access to both repositories.

Use `--dry-run` to preview a release. If publication is interrupted after tagging, run `npm run release -- --resume` from the release commit on main. To update just the formula for the latest release, run `npm run release:brew`.

## Checks

```bash
npm run check
npm test
cargo test --locked --manifest-path src-tauri/Cargo.toml
npm run build:app
```
