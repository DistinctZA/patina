// Cuts a Patina release: bumps the version everywhere, checks and builds the
// signed app, packages a signed .dmg, then commits, tags, pushes and
// publishes a GitHub Release with the .dmg attached.
//
//   npm run release -- minor          new features   (0.3.0 → 0.4.0)
//   npm run release -- patch          fixes only     (0.4.0 → 0.4.1)
//   npm run release -- major          big changes    (0.4.1 → 1.0.0)
//   npm run release -- minor --dry-run   show what would happen, change nothing
//
// Commit your work first: the script needs a clean tree on main.
import { execFileSync } from "node:child_process";
import { cpSync, existsSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

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

// Notes: every commit since the last release, with its description.
const lastTag = out("git", ["tag", "--list", "v*", "--sort=-v:refname"]).split("\n")[0] || null;
const range = lastTag ? `${lastTag}..HEAD` : "HEAD";
const log = out("git", ["log", range, "--reverse", "--format=### %s%n%n%b%n"])
  .replace(/\n*Co-Authored-By:.*$/gim, "")
  .trim();
if (!log) fail(`no commits since ${lastTag}`);
const notes = `${log}\n\n---\n\n**Install:** open the .dmg and drag Patina into Applications.`;

console.log(`release: ${current} → ${next} (${bump})${lastTag ? `, changes since ${lastTag}` : ""}\n`);
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

let dmg;
try {
  run("npm", ["run", "check"]);
  run("cargo", ["test", "--quiet"], { cwd: join(root, "src-tauri") });
  run("npm", ["run", "tauri", "build", "--", "--bundles", "app"]);

  // A plain disk image (app + Applications shortcut), signed like the app.
  // Built with hdiutil rather than Tauri's DMG step, which scripts Finder.
  const bundle = join(root, "src-tauri/target/release/bundle");
  const app = join(bundle, "macos/Patina.app");
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
run("gh", ["release", "create", tag, dmg, "--title", `Patina ${tag}`, "--notes-file", notesFile]);
console.log(`\nrelease: published ${tag}`);
