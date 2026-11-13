// Render queue (P1.M2.VW.2): at most two renders at a time, the most important first (visible
// pages before neighbours); a job whose page left the view is cancelled before or during work.

export interface Job {
  /** Lower runs first (0 = visible). */
  priority: () => number;
  /** Do the work; `signal` is aborted when the job is cancelled. */
  run: (signal: AbortSignal) => Promise<void>;
}

interface Entry {
  job: Job;
  ctrl: AbortController;
}

const MAX_RUNNING = 2;
const waiting: Entry[] = [];
let running = 0;

function pump(): void {
  while (running < MAX_RUNNING && waiting.length > 0) {
    waiting.sort((a, b) => a.job.priority() - b.job.priority());
    const e = waiting.shift();
    if (!e || e.ctrl.signal.aborted) continue;
    running += 1;
    void e.job
      .run(e.ctrl.signal)
      .catch(() => {})
      .finally(() => {
        running -= 1;
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
