import { useEffect, useRef, useState } from 'react';
import type { Project } from '../../ipc/contracts';
import { Icon } from '../chrome/Icon';

type Props = {
  title: string;
  project: Project | null;
  showSeparator: boolean;
};

export function SessionBar({ title, project, showSeparator }: Props) {
  const [open, setOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: PointerEvent) => {
      if (!menuRef.current?.contains(event.target as Node)) setOpen(false);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setOpen(false);
    };
    document.addEventListener('pointerdown', onPointerDown);
    document.addEventListener('keydown', onKeyDown);
    return () => {
      document.removeEventListener('pointerdown', onPointerDown);
      document.removeEventListener('keydown', onKeyDown);
    };
  }, [open]);

  return (
    <div className={`session-bar${showSeparator ? ' has-panel' : ''}`}>
      <div className="session-bar-title" ref={menuRef}>
        {project ? (
          <>
            <button
              type="button"
              className={`session-project-trigger${open ? ' is-open' : ''}`}
              aria-label={`Project ${project.name}`}
              aria-expanded={open}
              title={`Project ${project.name}`}
              onClick={() => setOpen((value) => !value)}
            >
              <Icon name="folder" />
            </button>
            <span className="session-title">{title}</span>
          </>
        ) : (
          <span className="session-title">{title}</span>
        )}
        {open && project ? (
          <div className="session-project-menu" role="menu" aria-label={`${project.name} project details`}>
            <div className="session-project-menu-heading">
              <Icon name="folder" />
              <strong>{project.name}</strong>
            </div>
            <span className="session-project-menu-label">Project folders</span>
            {project.rootPaths.length ? project.rootPaths.map((path) => (
              <div className="session-project-path" role="menuitem" key={path} title={path}>
                <Icon name="folder" />
                <span>{path}</span>
              </div>
            )) : (
              <div className="session-project-path is-empty" role="menuitem">No folders connected</div>
            )}
          </div>
        ) : null}
      </div>
    </div>
  );
}
