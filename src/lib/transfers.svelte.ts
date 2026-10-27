// Copies, moves and duplicates in progress, for the progress panel.
import { api } from "./api";

export type Job = {
  id: string;
  /** "Copying", "Moving"… (Rust updates it as the job goes). */
  verb: string;
  doneBytes: number;
  totalBytes: number;
  doneItems: number;
  totalItems: number;
  /** The file being worked on. */
  current: string;
  started: number;
  /** Quick jobs never show: the panel appears after a moment. */
  shown: boolean;
};

/** What Rust sends with each `transfer-progress` event. */
export type JobProgress = Omit<Job, "started" | "shown">;

const SHOW_AFTER_MS = 400;

class Transfers {
  jobs = $state<Job[]>([]);

  /** Runs `work` as a job with its own id; resolves or rejects with it. */
  async run(verb: string, work: (id: string) => Promise<void>): Promise<void> {
    const id = crypto.randomUUID();
    this.jobs.push({
      id,
      verb,
      doneBytes: 0,
      totalBytes: 0,
      doneItems: 0,
      totalItems: 0,
      current: "",
      started: Date.now(),
      shown: false,
    });
    const reveal = setTimeout(() => {
      const job = this.find(id);
      if (job) job.shown = true;
    }, SHOW_AFTER_MS);
    try {
      await work(id);
    } finally {
      clearTimeout(reveal);
      this.jobs = this.jobs.filter((j) => j.id !== id);
    }
  }

  update(progress: JobProgress) {
    const job = this.find(progress.id);
    if (job) Object.assign(job, progress);
  }

  cancel(id: string) {
    api.cancelTransfer(id);
  }

  private find(id: string) {
    return this.jobs.find((j) => j.id === id);
  }
}

export const transfers = new Transfers();

/** A cancelled job rejects with this; it isn't shown as an error. */
export const isCancelled = (err: unknown) => String(err).includes("Cancelled");
