export type FrameLane =
  | 'layout-read'
  | 'layout-write'
  | 'content'
  | 'scroll'
  | 'native-geometry'
  | 'idle';

const order: FrameLane[] = [
  'layout-read',
  'layout-write',
  'content',
  'scroll',
  'native-geometry',
  'idle',
];

export class FrameCoordinator {
  private scheduled = 0;
  private hidden = false;
  private readonly jobs = new Map<FrameLane, () => void>();

  constructor() {
    if (typeof document !== 'undefined') {
      this.hidden = document.hidden;
      document.addEventListener('visibilitychange', this.onVisibility);
    }
  }

  schedule(lane: FrameLane, job: () => void) {
    this.jobs.set(lane, job);
    if (this.hidden || this.scheduled) return;
    this.scheduled = requestAnimationFrame(this.flush);
  }

  flushNow() {
    if (this.scheduled) cancelAnimationFrame(this.scheduled);
    this.scheduled = 0;
    this.runJobs();
  }

  destroy() {
    if (this.scheduled) cancelAnimationFrame(this.scheduled);
    this.scheduled = 0;
    this.jobs.clear();
    if (typeof document !== 'undefined') {
      document.removeEventListener('visibilitychange', this.onVisibility);
    }
  }

  private readonly onVisibility = () => {
    this.hidden = document.hidden;
    if (!this.hidden && this.jobs.size && !this.scheduled) {
      this.scheduled = requestAnimationFrame(this.flush);
    }
  };

  private readonly flush = () => {
    this.scheduled = 0;
    this.runJobs();
  };

  private runJobs() {
    for (const lane of order) {
      const job = this.jobs.get(lane);
      if (!job) continue;
      this.jobs.delete(lane);
      job();
    }
  }
}

export const frameCoordinator = new FrameCoordinator();
