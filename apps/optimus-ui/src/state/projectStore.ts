import type { Project } from '../ipc/contracts';

const PROJECTS_KEY = 'optimus.ui.projects';
const PINS_KEY = 'optimus.react.sessionPins';
const ASSIGNMENTS_KEY = 'optimus.ui.sessionProjects';
const EXPANDED_KEY = 'optimus.ui.projectExpanded';
const PROJECT_SCHEMA_VERSION = 2;

type StoredProject = Partial<Project> & {
  path?: string;
};

type StoredProjectCatalog = {
  version: number;
  projects: StoredProject[];
};

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
  const stored = read<StoredProject[] | StoredProjectCatalog>(PROJECTS_KEY, []);
  const projects = Array.isArray(stored) ? stored : stored.projects || [];
  // Empty catalog stays empty — never invent a machine-specific path that looks
  // authorized but is absent from the Rust project-authority allowlist (#42).
  return projects
    .filter((project) => project.id && project.name)
    .map(normalizeProject);
}

export function saveProjects(projects: Project[]) {
  write(PROJECTS_KEY, {
    version: PROJECT_SCHEMA_VERSION,
    projects: projects.map(normalizeProject),
  });
}

export function createProject(name: string, path: string): Project {
  const now = new Date().toISOString();
  return {
    id: `project-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
    name,
    rootPaths: [path],
    primaryRoot: path,
    pinned: true,
    createdAt: now,
    updatedAt: now,
  };
}

export function addProjectRoot(project: Project, path: string): Project {
  const normalizedPath = path.trim();
  if (!normalizedPath || project.rootPaths.includes(normalizedPath)) return project;
  return {
    ...project,
    rootPaths: [...project.rootPaths, normalizedPath],
    primaryRoot: project.primaryRoot || normalizedPath,
    updatedAt: new Date().toISOString(),
  };
}

export function removeProjectRoot(project: Project, path: string): Project {
  const rootPaths = project.rootPaths.filter((rootPath) => rootPath !== path);
  return {
    ...project,
    rootPaths,
    primaryRoot:
      project.primaryRoot === path ? rootPaths[0] : project.primaryRoot,
    updatedAt: new Date().toISOString(),
  };
}

export function setPrimaryProjectRoot(project: Project, path: string): Project {
  if (!project.rootPaths.includes(path) || project.primaryRoot === path) return project;
  return {
    ...project,
    primaryRoot: path,
    updatedAt: new Date().toISOString(),
  };
}

function normalizeProject(project: StoredProject): Project {
  const legacyPath = typeof project.path === 'string' ? project.path.trim() : '';
  const roots = Array.isArray(project.rootPaths)
    ? project.rootPaths.filter((path): path is string => typeof path === 'string' && Boolean(path.trim()))
    : [];
  const rootPaths = Array.from(new Set((roots.length ? roots : legacyPath ? [legacyPath] : []).map((path) => path.trim())));
  const primaryRoot =
    typeof project.primaryRoot === 'string' && rootPaths.includes(project.primaryRoot)
      ? project.primaryRoot
      : rootPaths[0];
  return {
    id: String(project.id),
    name: String(project.name),
    rootPaths,
    ...(primaryRoot ? { primaryRoot } : {}),
    pinned: Boolean(project.pinned),
    ...(project.createdAt ? { createdAt: String(project.createdAt) } : {}),
    ...(project.updatedAt ? { updatedAt: String(project.updatedAt) } : {}),
  };
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
