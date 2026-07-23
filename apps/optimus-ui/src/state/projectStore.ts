import type { Project } from '../ipc/contracts';

const PROJECTS_KEY = 'optimus.ui.projects';
const PINS_KEY = 'optimus.react.sessionPins';
const ASSIGNMENTS_KEY = 'optimus.ui.sessionProjects';
const EXPANDED_KEY = 'optimus.ui.projectExpanded';

function read<T>(key: string, fallback: T): T {
  try {
    return JSON.parse(localStorage.getItem(key) || '') as T;
  } catch {
    return fallback;
  }
}

function write(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // Presentation-only storage is best effort.
  }
}

export function loadProjects(): Project[] {
  const projects = read<Array<Partial<Project>>>(PROJECTS_KEY, []);
  if (projects.length) {
    return projects
      .filter((project) => project.id && project.name)
      .map((project) => ({
        id: String(project.id),
        name: String(project.name),
        path: String(project.path || ''),
        pinned: Boolean(project.pinned),
      }));
  }
  return [
    {
      id: 'optimus-agent',
      name: 'Optimus Agent',
      path: '/home/mustbearnold/Projects/Optimus Agent',
      pinned: true,
    },
  ];
}

export function saveProjects(projects: Project[]) {
  write(PROJECTS_KEY, projects);
}

export function loadSessionPins(): string[] {
  return read<string[]>(PINS_KEY, []);
}

export function saveSessionPins(pins: string[]) {
  write(PINS_KEY, pins);
}

export function loadAssignments(): Record<string, string> {
  return read<Record<string, string>>(ASSIGNMENTS_KEY, {});
}

export function saveAssignments(assignments: Record<string, string>) {
  write(ASSIGNMENTS_KEY, assignments);
}

export function loadExpanded(): Record<string, boolean> {
  return read<Record<string, boolean>>(EXPANDED_KEY, {
    'optimus-agent': true,
    inbox: true,
  });
}

export function saveExpanded(expanded: Record<string, boolean>) {
  write(EXPANDED_KEY, expanded);
}
