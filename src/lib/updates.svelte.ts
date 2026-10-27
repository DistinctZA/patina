// New versions: checks GitHub for a newer Patina, then downloads, installs
// and restarts. The download is checked against the key in tauri.conf.json,
// so only builds signed by the release script are ever installed.
import { getVersion } from "@tauri-apps/api/app";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";

export type UpdateStatus =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "current" }
  | { kind: "available"; update: Update }
  | { kind: "downloading"; update: Update; done: number; total: number | null }
  | { kind: "restarting" }
  | { kind: "error"; message: string };

/** How often to look again while Patina stays open. */
const RECHECK_MS = 12 * 60 * 60 * 1000;
/** Wait a moment after launch, so the check never slows startup. */
const FIRST_CHECK_MS = 5000;

class Updates {
  /** This copy's version, e.g. "0.6.0". */
  version = $state("");
  status = $state.raw<UpdateStatus>({ kind: "idle" });
  private timers: ReturnType<typeof setTimeout>[] = [];

  constructor() {
    getVersion().then((v) => (this.version = v));
  }

  get available() {
    return this.status.kind === "available";
  }

  /**
   * Looks for a newer version. `quiet` (the automatic check) keeps errors
   * out of sight, since a missed check on a train isn't worth a message.
   */
  async check(quiet = false) {
    const busy = ["checking", "downloading", "restarting"].includes(this.status.kind);
    if (busy || (quiet && this.status.kind === "available")) return;
    this.status = { kind: "checking" };
    try {
      const update = await check();
      this.status = update ? { kind: "available", update } : { kind: "current" };
    } catch (err) {
      this.status = quiet ? { kind: "idle" } : { kind: "error", message: describe(err) };
    }
  }

  /** Downloads and installs the update found by `check`, then restarts. */
  async install() {
    if (this.status.kind !== "available") return;
    const update = this.status.update;
    this.status = { kind: "downloading", update, done: 0, total: null };
    try {
      await update.downloadAndInstall((ev) => {
        if (this.status.kind !== "downloading") return;
        if (ev.event === "Started") this.status = { ...this.status, total: ev.data.contentLength ?? null };
        if (ev.event === "Progress") this.status = { ...this.status, done: this.status.done + ev.data.chunkLength };
      });
      this.status = { kind: "restarting" };
      await relaunch();
    } catch (err) {
      this.status = { kind: "error", message: describe(err) };
    }
  }

  /** Turns the automatic check (at launch, then twice a day) on or off. */
  auto(on: boolean) {
    this.timers.forEach(clearTimeout);
    this.timers = [];
    if (!on) return;
    this.timers.push(setTimeout(() => this.check(true), FIRST_CHECK_MS));
    this.timers.push(setInterval(() => this.check(true), RECHECK_MS));
  }
}

/** The updater's errors are written for developers; say it plainly. */
function describe(err: unknown): string {
  const text = String(err);
  if (/signature/i.test(text)) return "The download didn't match Patina's signature, so it wasn't installed.";
  if (/permission|denied|read-only/i.test(text)) return "Patina couldn't replace itself. Move it into Applications and try again.";
  if (/platform/i.test(text)) return "There's no ready-made update for this kind of Mac. Build the new version from source.";
  return "Couldn't reach the update server. Check your internet connection and try again.";
}

export const updates = new Updates();
