// Notarizes Patina builds with Apple, so downloads open without the
// "Apple could not verify Patina" warning. Uses a login saved in the
// keychain by `xcrun notarytool store-credentials`, so no password is ever
// in a file or an environment variable.
//
//   node scripts/notarize.mjs Patina.app Patina_1.0.0.dmg   notarize and staple
//
// One-time setup on each Mac you release from (it asks for an app-specific
// password, made at https://account.apple.com → Sign-In and Security):
//
//   xcrun notarytool store-credentials patina-notary --apple-id YOU@EXAMPLE.COM --team-id TEAMID
import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, join } from "node:path";
import { fileURLToPath } from "node:url";

export const profile = process.env.PATINA_NOTARY_PROFILE || "patina-notary";

const out = (cmd, args) => execFileSync(cmd, args, { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();

/** True if the saved login exists and Apple accepts it. */
export function notaryReady() {
  try {
    out("xcrun", ["notarytool", "history", "--keychain-profile", profile, "--output-format", "json"]);
    return true;
  } catch {
    return false;
  }
}

/** Sends a .app or .dmg to Apple, waits for the verdict, and staples the ticket to it. */
export function notarize(path) {
  // An app goes up as a zip; a disk image goes up as it is.
  let upload = path;
  let scratch = null;
  if (path.endsWith(".app")) {
    scratch = mkdtempSync(join(tmpdir(), "patina-notarize-"));
    upload = join(scratch, `${basename(path)}.zip`);
    execFileSync("ditto", ["-c", "-k", "--keepParent", path, upload]);
  }
  try {
    console.log(`notarize: sending ${basename(path)} to Apple (usually a few minutes)…`);
    const result = JSON.parse(
      out("xcrun", ["notarytool", "submit", upload, "--keychain-profile", profile, "--wait", "--output-format", "json"]),
    );
    if (result.status !== "Accepted") {
      let log = "";
      try {
        log = out("xcrun", ["notarytool", "log", result.id, "--keychain-profile", profile]);
      } catch {}
      throw new Error(`Apple returned "${result.status}" for ${basename(path)}\n${log}`);
    }
  } finally {
    if (scratch) rmSync(scratch, { recursive: true, force: true });
  }
  execFileSync("xcrun", ["stapler", "staple", "-q", path]);
  execFileSync("xcrun", ["stapler", "validate", "-q", path]);
  console.log(`notarize: ${basename(path)} accepted and stapled`);
}

/** Throws unless macOS would open this download without a warning. */
export function assertGatekeeperAccepts(path) {
  const args = path.endsWith(".dmg")
    ? ["--assess", "--type", "open", "--context", "context:primary-signature", "-v", path]
    : ["--assess", "--type", "execute", "-v", path];
  // spctl exits non-zero (so this throws) if macOS would block it.
  execFileSync("spctl", args, { stdio: ["ignore", "ignore", "pipe"] });
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const files = process.argv.slice(2);
  if (!files.length) {
    console.error("usage: node scripts/notarize.mjs <Patina.app | Patina.dmg>…");
    process.exit(1);
  }
  if (!notaryReady()) {
    console.error(`notarize: no working keychain login named "${profile}". See the setup line at the top of this file.`);
    process.exit(1);
  }
  for (const file of files) {
    notarize(file);
    assertGatekeeperAccepts(file);
  }
}
