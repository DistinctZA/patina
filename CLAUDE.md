# Patina

A dual-pane file manager for macOS: Rust (Tauri 2) backend in `src-tauri/`,
Svelte 5 UI in `src/`. See README.md for features and layout.

## Releasing: every push is a release

Don't `git push` on its own. Commit the work, then run:

```bash
npm run release -- minor   # new features: 0.3.0 → 0.4.0
npm run release -- patch   # fixes only:   0.4.0 → 0.4.1
```

The script bumps the version in package.json, package-lock.json,
tauri.conf.json, Cargo.toml, Cargo.lock and the README badge; runs the checks
and tests; builds the signed app and a signed .dmg; then commits
"Release vX.Y.Z", tags it, pushes, and publishes a GitHub Release with the
.dmg and notes taken from the commits since the last tag. If any check or
build step fails, it undoes the version change and stops before committing.

Try `npm run release -- minor --dry-run` first to see the next version and notes.

Release notes are built from commit messages, so write commits for a reader:
a short subject, then a body listing what changed.

Each release also publishes a signed update (`Patina_X.Y.Z_aarch64.app.tar.gz`)
and `latest.json`, which the in-app updater reads. They're signed with
`~/.tauri/patina_updater.key` (plus `.key.password`), kept outside the repo;
copy both to any other Mac you release from, or the script stops. Losing the
key means installed copies can't update themselves any more.

Notarization is optional: if a keychain login named `patina-notary` exists
(see `scripts/notarize.mjs`), releases are notarized so downloads open
without Gatekeeper's warning.

## Checks

```bash
npm run check                      # Svelte/TypeScript
cd src-tauri && cargo test         # Rust
```
