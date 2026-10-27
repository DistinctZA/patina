// Cuts a Patina release: bumps the version everywhere, checks and builds the
// signed app, packages a signed .dmg (both notarized by Apple if a notary
// login is saved), then commits, tags, pushes and publishes a GitHub Release
// with the .dmg and the signed update that Patina's updater installs.
//
//   npm run release -- minor          new features   (0.3.0 → 0.4.0)
//   npm run release -- patch          fixes only     (0.4.0 → 0.4.1)
//   npm run release -- major          big changes    (0.4.1 → 1.0.0)
//   npm run release -- minor --dry-run   show what would happen, change nothing
//
// Commit your work first: the script needs a clean tree on main. Notarizing
// is optional and needs a saved Apple login; see scripts/notarize.mjs.
// Updates are signed with ~/.tauri/patina_updater.key (and .key.password);
// copy both to any other Mac you release from. Without that key, existing
// copies of Patina can't install new versions.
import { execFileSync } from "node:child_process";
import { cpSync, existsSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { basename, dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { assertGatekeeperAccepts, notarize, notaryReady, profile } from "./notarize.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const bump = args.find((a) => !a.startsWith("--")) ?? "patch";
const dryRun = args.includes("--dry-run");

const run = (cmd, cmdArgs, opts = {}) => execFileSync(cmd, cmdArgs, { cwd: root, stdio: "inherit", ...opts });
const out = (cmd, cmdArgs) => execFileSync(cmd, cmdArgs, { cwd: root, encoding: "utf8" }).trim();
const read = (file) => readFileSync(join(root, file), "utf8");
const write = (file, text) => writeFileSync(join(root, file), text);
const fail = (message) => {
  console.error(`release: ${message}`);
  process.exit(1);
};

// ── Preflight ────────────────────────────────────────────────────────────

if (!["patch", "minor", "major"].includes(bump)) fail(`unknown bump "${bump}" (use patch, minor or major)`);
if (out("git", ["branch", "--show-current"]) !== "main") fail("releases are cut from main");
if (out("git", ["status", "--porcelain"])) fail("commit or stash your changes first");
run("git", ["fetch", "--quiet", "--tags", "origin"]);
if (out("git", ["rev-list", "--count", "HEAD..origin/main"]) !== "0") fail("main is behind origin; pull first");

const current = JSON.parse(read("package.json")).version;
const [major, minor, patch] = current.split(".").map(Number);
const next = { major: `${major + 1}.0.0`, minor: `${major}.${minor + 1}.0`, patch: `${major}.${minor}.${patch + 1}` }[bump];
const tag = `v${next}`;
if (out("git", ["tag", "--list", tag])) fail(`${tag} already exists`);
// Without notarization, downloads open with "Apple could not verify Patina"
// until the user clicks Open Anyway, so it's done whenever a login is saved.
const notaryOk = notaryReady();
// The key Patina's updater checks every download against.
const updaterKey = join(homedir(), ".tauri/patina_updater.key");
if (!existsSync(updaterKey) || !existsSync(`${updaterKey}.password`)) {
  fail(`no update signing key at ${updaterKey} (copy it and its .password file from the Mac that made it)`);
}

// Notes: every commit since the last release, with its description.
const lastTag = out("git", ["tag", "--list", "v*", "--sort=-v:refname"]).split("\n")[0] || null;
const range = lastTag ? `${lastTag}..HEAD` : "HEAD";
const log = out("git", ["log", range, "--reverse", "--format=### %s%n%n%b%n"])
  .replace(/\n*Co-Authored-By:.*$/gim, "")
  .trim();
if (!log) fail(`no commits since ${lastTag}`);
const notes = `${log}\n\n---\n\n**Install:** open the .dmg and drag Patina into Applications. Already have Patina? It updates itself.`;

console.log(`release: ${current} → ${next} (${bump})${lastTag ? `, changes since ${lastTag}` : ""}`);
console.log(`release: notarization ${notaryOk ? `on (keychain login "${profile}")` : "off, downloads will need Open Anyway (setup: scripts/notarize.mjs)"}\n`);
if (dryRun) {
  console.log(notes);
  console.log("\nrelease: dry run, nothing changed.");
  process.exit(0);
}

// ── Bump the version everywhere it's written down ────────────────────────

const replaceOnce = (file, pattern, replacement) => {
  const text = read(file);
  if (!pattern.test(text)) fail(`couldn't find the version in ${file}`);
  write(file, text.replace(pattern, replacement));
};

const pkg = JSON.parse(read("package.json"));
pkg.version = next;
write("package.json", JSON.stringify(pkg, null, 2) + "\n");

const lock = JSON.parse(read("package-lock.json"));
lock.version = next;
lock.packages[""].version = next;
write("package-lock.json", JSON.stringify(lock, null, 2) + "\n");

const conf = JSON.parse(read("src-tauri/tauri.conf.json"));
replaceOnce("src-tauri/tauri.conf.json", /"version": "[^"]+"/, `"version": "${next}"`);
replaceOnce("src-tauri/Cargo.toml", /^version = ".*"$/m, `version = "${next}"`);
replaceOnce("src-tauri/Cargo.lock", /(name = "patina"\nversion = )".*"/, `$1"${next}"`);
replaceOnce("README.md", /badge\/version-[\d.]+-/, `badge/version-${next}-`);

const bumped = ["package.json", "package-lock.json", "src-tauri/tauri.conf.json", "src-tauri/Cargo.toml", "src-tauri/Cargo.lock", "README.md"];
const undo = () => run("git", ["checkout", "--", ...bumped]);

// ── Check, build, package ────────────────────────────────────────────────

let dmg, update, manifest;
try {
  run("npm", ["run", "check"]);
  run("cargo", ["test", "--quiet"], { cwd: join(root, "src-tauri") });
  // Also builds the update (Patina.app.tar.gz) and its signature. Only
  // releases do this, so building from source needs no key.
  run("npm", ["run", "tauri", "build", "--", "--bundles", "app", "--config", JSON.stringify({ bundle: { createUpdaterArtifacts: true } })], {
    env: {
      ...process.env,
      TAURI_SIGNING_PRIVATE_KEY: readFileSync(updaterKey, "utf8"),
      TAURI_SIGNING_PRIVATE_KEY_PASSWORD: readFileSync(`${updaterKey}.password`, "utf8").trim(),
    },
  });
  const bundle = join(root, "src-tauri/target/release/bundle");
  const app = join(bundle, "macos/Patina.app");
  if (notaryOk) notarize(app);

  // A plain disk image (app + Applications shortcut), signed like the app.
  // Built with hdiutil rather than Tauri's DMG step, which scripts Finder.
  const identity = conf.bundle.macOS.signingIdentity;
  const stage = mkdtempSync(join(tmpdir(), "patina-dmg-"));
  dmg = join(bundle, `Patina_${next}.dmg`);
  run("ditto", [app, join(stage, "Patina.app")]);
  symlinkSync("/Applications", join(stage, "Applications"));
  if (existsSync(dmg)) rmSync(dmg);
  run("hdiutil", ["create", "-quiet", "-volname", `Patina ${next}`, "-srcfolder", stage, "-format", "UDZO", dmg]);
  rmSync(stage, { recursive: true, force: true });
  run("codesign", ["--sign", identity, "--timestamp", dmg]);
  run("codesign", ["--verify", dmg]);
  if (notaryOk) {
    notarize(dmg);
    // The check a downloaded copy goes through on someone else's Mac.
    assertGatekeeperAccepts(app);
    assertGatekeeperAccepts(dmg);
  }

  // What the updater fetches: latest.json from the newest release names the
  // version, the notes, and the signed archive for each kind of Mac.
  const arch = { arm64: "aarch64", x64: "x86_64" }[process.arch];
  update = join(bundle, `Patina_${next}_${arch}.app.tar.gz`);
  cpSync(join(bundle, "macos/Patina.app.tar.gz"), update);
  const repo = out("gh", ["repo", "view", "--json", "nameWithOwner", "-q", ".nameWithOwner"]);
  manifest = join(bundle, "latest.json");
  writeFileSync(
    manifest,
    JSON.stringify(
      {
        version: next,
        notes: log,
        pub_date: new Date().toISOString(),
        platforms: {
          [`darwin-${arch}`]: {
            signature: readFileSync(join(bundle, "macos/Patina.app.tar.gz.sig"), "utf8").trim(),
            url: `https://github.com/${repo}/releases/download/${tag}/${basename(update)}`,
          },
        },
      },
      null,
      2,
    ) + "\n",
  );
} catch (err) {
  undo();
  fail(`stopped before committing; version changes undone (${err.message})`);
}

// ── Commit, tag, push, publish ───────────────────────────────────────────

run("git", ["add", ...bumped]);
run("git", ["commit", "--quiet", "-m", `Release ${tag}`]);
run("git", ["tag", "-a", tag, "-m", `Patina ${tag}`]);
run("git", ["push", "--quiet", "origin", "main", tag]);

const notesFile = join(mkdtempSync(join(tmpdir(), "patina-notes-")), "notes.md");
writeFileSync(notesFile, notes);
run("gh", ["release", "create", tag, dmg, update, manifest, "--title", `Patina ${tag}`, "--notes-file", notesFile]);
console.log(`\nrelease: published ${tag}`);
