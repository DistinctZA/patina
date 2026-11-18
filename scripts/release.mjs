// Cut a source release from main. Use --resume to finish publishing a tagged
// release after a network failure, or --dry-run to preview its version and notes.
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const run = (cmd, args, cwd = root) => execFileSync(cmd, args, { cwd, stdio: "inherit" });
const out = (cmd, args) => execFileSync(cmd, args, { cwd: root, encoding: "utf8" }).trim();
const read = (path) => readFileSync(join(root, path), "utf8");
const write = (path, content) => writeFileSync(join(root, path), content);
const fail = (message) => { throw new Error(message); };
const args = process.argv.slice(2);
const bump = args.find((arg) => !arg.startsWith("--")) ?? "patch";
const resume = args.includes("--resume");
const dryRun = args.includes("--dry-run");

if (!["patch", "minor", "major"].includes(bump)) fail(`Unknown version increment: ${bump}`);
if (out("git", ["branch", "--show-current"]) !== "main") fail("Release from main.");
if (out("git", ["status", "--porcelain"])) fail("Commit or stash changes before releasing.");
run("git", ["fetch", "--quiet", "--tags", "origin"]);
if (out("git", ["rev-list", "--count", "HEAD..origin/main"]) !== "0") fail("Pull the changes on origin/main first.");

const pkg = JSON.parse(read("package.json"));
const [major, minor, patch] = pkg.version.split(".").map(Number);
const next = resume ? pkg.version : {
  major: `${major + 1}.0.0`, minor: `${major}.${minor + 1}.0`, patch: `${major}.${minor}.${patch + 1}`,
}[bump];
const tag = `v${next}`;
if (resume) {
  if (out("git", ["tag", "--points-at", "HEAD", "--list", tag]) !== tag) fail(`Check out the release commit for ${tag} to resume.`);
} else if (out("git", ["tag", "--list", tag])) {
  fail(`${tag} already exists.`);
}

const end = resume ? `${tag}^` : "HEAD";
const previous = out("git", ["tag", "--merged", end, "--list", "v*", "--sort=-v:refname"]).split("\n")[0];
const history = out("git", ["log", previous ? `${previous}..${end}` : end, "--reverse", "--format=### %s%n%n%b%n"])
  .replace(/\n*Co-Authored-By:.*$/gim, "").trim();
if (!history) fail(`No changes since ${previous}.`);
const repo = out("gh", ["repo", "view", "--json", "nameWithOwner", "-q", ".nameWithOwner"]);
const notes = `${history}\n\n[Build or install Patina](https://github.com/${repo}#install-with-homebrew).\n`;
console.log(`Source release ${tag}`);
if (dryRun) { console.log(notes); process.exit(0); }

if (!resume) {
  const files = ["package.json", "package-lock.json", "src-tauri/tauri.conf.json", "src-tauri/Cargo.toml", "src-tauri/Cargo.lock", "README.md"];
  const replace = (file, pattern, replacement) => {
    const original = read(file);
    if (!pattern.test(original)) fail(`Couldn't find the version in ${file}.`);
    write(file, original.replace(pattern, replacement));
  };
  try {
    pkg.version = next;
    write("package.json", JSON.stringify(pkg, null, 2) + "\n");
    const lock = JSON.parse(read("package-lock.json"));
    lock.version = lock.packages[""].version = next;
    write("package-lock.json", JSON.stringify(lock, null, 2) + "\n");
    replace("src-tauri/tauri.conf.json", /"version": "[^"]+"/, `"version": "${next}"`);
    replace("src-tauri/Cargo.toml", /^version = ".*"$/m, `version = "${next}"`);
    replace("src-tauri/Cargo.lock", /(name = "patina"\nversion = )"[^"]+"/, `$1"${next}"`);
    replace("README.md", /badge\/version-[\d.]+-/, `badge/version-${next}-`);
    run("npm", ["run", "check"]);
    run("npm", ["test"]);
    run("cargo", ["test", "--locked", "--quiet"], join(root, "src-tauri"));
    run("npm", ["run", "build:app"]);
  } catch (err) {
    run("git", ["restore", "--", ...files]);
    throw err;
  }
  run("git", ["add", "--", ...files]);
  run("git", ["commit", "--quiet", "-m", `Release ${tag}`]);
  run("git", ["tag", "-a", tag, "-m", `Patina ${tag}`]);
}

run("git", ["push", "--atomic", "origin", "main", `refs/tags/${tag}`]);
const published = JSON.parse(out("gh", ["release", "list", "--repo", repo, "--limit", "100", "--exclude-drafts", "--json", "tagName"]));
if (!published.some((release) => release.tagName === tag)) {
  const scratch = mkdtempSync(join(tmpdir(), "patina-release-"));
  try {
    const path = join(scratch, "notes.md");
    writeFileSync(path, notes);
    run("gh", ["release", "create", tag, "--repo", repo, "--verify-tag", "--title", `Patina ${tag}`, "--notes-file", path, "--latest"]);
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
}
if (repo.toLowerCase() === "distinctza/patina") run("npm", ["run", "release:brew"]);
console.log(`Published source release ${tag}.`);
