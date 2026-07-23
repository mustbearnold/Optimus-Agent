import { describe, expect, it, vi } from 'vitest';
import { FrameCoordinator } from './frameCoordinator';

describe('FrameCoordinator', () => {
  it('converges thousands of updates to one frame and the latest value', () => {
    let callback: FrameRequestCallback | undefined;
    const raf = vi
      .spyOn(globalThis, 'requestAnimationFrame')
      .mockImplementation((next) => {
        callback = next;
        return 7;
      });
    vi.spyOn(globalThis, 'cancelAnimationFrame').mockImplementation(() => undefined);
    const coordinator = new FrameCoordinator();
    let value = -1;

    for (let index = 0; index < 4_000; index += 1) {
      coordinator.schedule('content', () => {
        value = index;
      });
    }

    expect(raf).toHaveBeenCalledTimes(1);
    callback?.(16.7);
    expect(value).toBe(3_999);
    coordinator.destroy();
  });

  it('runs layout reads before writes and native geometry', () => {
    const coordinator = new FrameCoordinator();
    const order: string[] = [];
    coordinator.schedule('native-geometry', () => order.push('geometry'));
    coordinator.schedule('layout-write', () => order.push('write'));
    coordinator.schedule('layout-read', () => order.push('read'));
    coordinator.flushNow();
    expect(order).toEqual(['read', 'write', 'geometry']);
    coordinator.destroy();
  });
});
