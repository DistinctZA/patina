// Publish the formula for the latest source release. Safe to retry on its own.
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";

const project = "DistinctZA/patina";
const tap = "DistinctZA/homebrew-tap";
const latest = () => execFileSync("gh", ["release", "view", "--repo", project, "--json", "tagName", "-q", ".tagName"], { encoding: "utf8" }).trim();
const tag = latest();
if (!/^v\d+\.\d+\.\d+$/.test(tag)) throw new Error(`Unexpected release tag: ${tag}`);
const scratch = mkdtempSync(join(tmpdir(), "patina-homebrew-"));
try {
  const archive = join(scratch, "source.tar.gz");
  const url = `https://github.com/${project}/archive/refs/tags/${tag}.tar.gz`;
  execFileSync("curl", ["--fail", "--silent", "--show-error", "--location", "--retry", "3", "--retry-all-errors", "--output", archive, url]);
  const digest = createHash("sha256").update(readFileSync(archive)).digest("hex");
  const template = readFileSync(new URL("../packaging/patina.rb.in", import.meta.url), "utf8");
  const formula = template.replace("@VERSION@", tag.slice(1)).replace("@SHA256@", digest);
  const checkout = join(scratch, "tap");
  execFileSync("gh", ["repo", "clone", tap, checkout, "--", "--depth=1"], { stdio: "inherit" });
  const path = join(checkout, "Formula", "patina.rb");
  if (!existsSync(path) || readFileSync(path, "utf8") !== formula) {
    mkdirSync(dirname(path), { recursive: true });
    writeFileSync(path, formula);
    const git = (args) => execFileSync("git", args, { cwd: checkout, stdio: "inherit" });
    git(["add", "--", "Formula/patina.rb"]);
    git(["commit", "-m", `patina ${tag.slice(1)}`]);
    if (latest() !== tag) throw new Error("A newer release appeared. Run npm run release:brew again.");
    git(["push", "origin", "HEAD:main"]);
  }
  console.log(`Homebrew formula ready for ${tag}.`);
} finally {
  rmSync(scratch, { recursive: true, force: true });
}
