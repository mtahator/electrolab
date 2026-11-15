// Render queue (P1.M2.VW.2): the most important first (visible pages before neighbours); a job
// whose page left the view is cancelled before or during work.
//
// Two lanes (design language §17.2, review 2026-11-15): background jobs (previews of pages
// ahead) have slots of their own, so they can never take the slots the page in view needs —
// the engine serves them only when it has nothing more important to do, and they may wait.

export interface Job {
  /** Lower runs first (0 = visible). */
  priority: () => number;
  /** Background work (previews ahead): its own slots, never blocking visible pages. */
  background?: boolean;
  /** Do the work; `signal` is aborted when the job is cancelled. */
  run: (signal: AbortSignal) => Promise<void>;
}

interface Entry {
  job: Job;
  ctrl: AbortController;
}

const SLOTS = { interactive: 2, background: 2 };
const waiting: Entry[] = [];
const running = { interactive: 0, background: 0 };

function lane(e: Entry): "interactive" | "background" {
  return e.job.background ? "background" : "interactive";
}

function pump(): void {
  waiting.sort((a, b) => a.job.priority() - b.job.priority());
  for (let i = 0; i < waiting.length; ) {
    const e = waiting[i] as Entry;
    const l = lane(e);
    if (e.ctrl.signal.aborted) {
      waiting.splice(i, 1);
      continue;
    }
    if (running[l] >= SLOTS[l]) {
      i += 1;
      continue;
    }
    waiting.splice(i, 1);
    running[l] += 1;
    void e.job
      .run(e.ctrl.signal)
      .catch(() => {})
      .finally(() => {
        running[l] -= 1;
        pump();
      });
  }
}

/** Queue a job; returns a function that cancels it. */
export function enqueue(job: Job): () => void {
  const e: Entry = { job, ctrl: new AbortController() };
  waiting.push(e);
  queueMicrotask(pump);
  return () => {
    e.ctrl.abort();
    const i = waiting.indexOf(e);
    if (i >= 0) waiting.splice(i, 1);
  };
}

/** Jobs not started yet (for tests and the debug overlay). */
export function pending(): number {
  return waiting.length;
}
