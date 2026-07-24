import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  addProjectRoot,
  loadProjects,
  removeProjectRoot,
  saveProjects,
  setPrimaryProjectRoot,
} from './projectStore';
import type { Project } from '../ipc/contracts';

describe('multi-folder project catalog', () => {
  beforeEach(() => {
    localStorage.clear();
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-07-24T00:00:00.000Z'));
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('migrates the legacy single-path schema without losing identity', () => {
    localStorage.setItem('optimus.ui.projects', JSON.stringify([
      {
        id: 'legacy',
        name: 'Legacy project',
        path: '/workspace/legacy',
        pinned: true,
      },
    ]));

    expect(loadProjects()).toEqual([
      {
        id: 'legacy',
        name: 'Legacy project',
        rootPaths: ['/workspace/legacy'],
        primaryRoot: '/workspace/legacy',
        pinned: true,
      },
    ]);
  });

  it('deduplicates sources and moves primary ownership deterministically', () => {
    const project = fixtureProject();
    const withSecondRoot = addProjectRoot(project, '/workspace/docs');
    expect(addProjectRoot(withSecondRoot, '/workspace/docs')).toBe(withSecondRoot);
    expect(withSecondRoot.rootPaths).toEqual(['/workspace/app', '/workspace/docs']);

    const primaryMoved = setPrimaryProjectRoot(withSecondRoot, '/workspace/docs');
    expect(primaryMoved.primaryRoot).toBe('/workspace/docs');

    const removed = removeProjectRoot(primaryMoved, '/workspace/docs');
    expect(removed.rootPaths).toEqual(['/workspace/app']);
    expect(removed.primaryRoot).toBe('/workspace/app');
  });

  it('persists a versioned rootPaths catalog', () => {
    saveProjects([fixtureProject()]);
    expect(JSON.parse(localStorage.getItem('optimus.ui.projects') || '{}')).toMatchObject({
      version: 2,
      projects: [{
        id: 'project',
        rootPaths: ['/workspace/app'],
        primaryRoot: '/workspace/app',
      }],
    });
  });
});

function fixtureProject(): Project {
  return {
    id: 'project',
    name: 'Project',
    rootPaths: ['/workspace/app'],
    primaryRoot: '/workspace/app',
    pinned: true,
  };
}
