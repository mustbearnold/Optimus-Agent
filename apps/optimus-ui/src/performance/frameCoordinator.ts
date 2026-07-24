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
  private readonly jobs = new Map<FrameLane, Map<string, () => void>>();

  constructor() {
    if (typeof document !== 'undefined') {
      this.hidden = document.hidden;
      document.addEventListener('visibilitychange', this.onVisibility);
    }
  }

  schedule(lane: FrameLane, job: () => void) {
    this.scheduleKeyed(lane, 'default', job);
  }

  scheduleKeyed(lane: FrameLane, key: string, job: () => void) {
    const laneJobs = this.jobs.get(lane) || new Map<string, () => void>();
    laneJobs.set(key, job);
    this.jobs.set(lane, laneJobs);
    if (this.hidden || this.scheduled) return;
    this.scheduled = requestAnimationFrame(this.flush);
  }

  cancelKeyed(lane: FrameLane, key: string) {
    const laneJobs = this.jobs.get(lane);
    if (!laneJobs) return;
    laneJobs.delete(key);
    if (!laneJobs.size) this.jobs.delete(lane);
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
      const laneJobs = this.jobs.get(lane);
      if (!laneJobs) continue;
      this.jobs.delete(lane);
      laneJobs.forEach((job) => job());
    }
  }
}

export const frameCoordinator = new FrameCoordinator();
