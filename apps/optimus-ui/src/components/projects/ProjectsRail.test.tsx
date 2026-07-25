import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { ProjectsRail } from './ProjectsRail';

describe('ProjectsRail session actions', () => {
  it('restores focus to the exact action trigger after Escape and selection', async () => {
    const user = userEvent.setup();
    const onTogglePin = vi.fn();
    render(
      <ProjectsRail
        collapsed={false}
        sessions={[{ id: 'session-1', title: 'Audit session', message_count: 2 }]}
        projects={[{ id: 'project-1', name: 'Optimus Agent', rootPaths: ['/workspace'] }]}
        assignments={{ 'session-1': 'project-1' }}
        expanded={{ 'project-1': true }}
        selectedSessionId="session-1"
        sessionIndicators={{}}
        route="work"
        showArchived={false}
        onShowArchived={vi.fn()}
        onSearch={vi.fn()}
        onSelectSession={vi.fn()}
        onNewSession={vi.fn()}
        onRoute={vi.fn()}
        onAddProject={vi.fn()}
        onManageProject={vi.fn()}
        onToggleProject={vi.fn()}
        onTogglePin={onTogglePin}
        onToggleArchive={vi.fn()}
        onAssign={vi.fn()}
        onRename={vi.fn()}
        onDelete={vi.fn()}
        onSettings={vi.fn()}
      />
    );

    const trigger = screen.getByRole('button', { name: 'Actions for Audit session' });
    await user.click(trigger);
    expect(screen.getByRole('menu', { name: 'Actions for Audit session' })).toBeInTheDocument();
    await user.keyboard('{Escape}');
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
    await waitForFocus(trigger);

    await user.click(trigger);
    await user.click(screen.getByRole('menuitem', { name: 'Pin session' }));
    expect(onTogglePin).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
    await waitForFocus(trigger);
  });

  it('opens the session control menu from a row context click', async () => {
    render(
      <ProjectsRail
        collapsed={false}
        sessions={[{ id: 'session-1', title: 'Audit session', message_count: 2 }]}
        projects={[{ id: 'project-1', name: 'Optimus Agent', rootPaths: ['/workspace'] }]}
        assignments={{ 'session-1': 'project-1' }}
        expanded={{ 'project-1': true }}
        selectedSessionId="session-1"
        sessionIndicators={{}}
        route="work"
        showArchived={false}
        onShowArchived={vi.fn()}
        onSearch={vi.fn()}
        onSelectSession={vi.fn()}
        onNewSession={vi.fn()}
        onRoute={vi.fn()}
        onAddProject={vi.fn()}
        onManageProject={vi.fn()}
        onToggleProject={vi.fn()}
        onTogglePin={vi.fn()}
        onToggleArchive={vi.fn()}
        onAssign={vi.fn()}
        onRename={vi.fn()}
        onDelete={vi.fn()}
        onSettings={vi.fn()}
      />
    );

    const row = screen.getByTitle('Audit session').closest('.session-row');
    expect(row).not.toBeNull();
    expect(fireEvent.contextMenu(row!, { clientX: 73, clientY: 119 })).toBe(false);
    const menu = screen.getByRole('menu', { name: 'Actions for Audit session' });
    expect(menu).toBeInTheDocument();
    expect(menu).toHaveStyle({ position: 'fixed', left: '73px', top: '119px', animation: 'none' });
    await waitForFocus(screen.getByRole('button', { name: 'Actions for Audit session' }));
  });
});

async function waitForFocus(element: HTMLElement) {
  await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
  expect(element).toHaveFocus();
}
