import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { formatSessionAge, ProjectsRail } from './ProjectsRail';

describe('ProjectsRail session actions', () => {
  it('formats idle age only in elapsed minutes or hours', () => {
    const now = Date.parse('2026-07-25T12:00:00.000Z');
    expect(formatSessionAge('2026-07-25T11:58:30.000Z', now)).toBe('1m');
    expect(formatSessionAge('2026-07-25T09:30:00.000Z', now)).toBe('2h');
    expect(formatSessionAge('2026-07-23T11:00:00.000Z', now)).toBe('49h');
  });

  it('keeps search inline and exposes the direct new-thread action', async () => {
    const user = userEvent.setup();
    const onSearch = vi.fn();
    const onNewSession = vi.fn();
    render(
      <ProjectsRail
        collapsed={false}
        sessions={[]}
        projects={[]}
        assignments={{}}
        expanded={{}}
        selectedSessionId={null}
        sessionIndicators={{}}
        onSearch={onSearch}
        onSelectSession={vi.fn()}
        onNewSession={onNewSession}
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

    await user.type(screen.getByRole('searchbox', { name: /Search threads/ }), 'runtime');
    expect(onSearch).toHaveBeenLastCalledWith('runtime');
    await user.click(screen.getByRole('button', { name: 'New thread' }));
    expect(onNewSession).toHaveBeenCalledTimes(1);
  });

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
        onSearch={vi.fn()}
        onSelectSession={vi.fn()}
        onNewSession={vi.fn()}
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

    const trigger = screen.getByTitle('Audit session');
    fireEvent.keyDown(trigger, { key: 'F10', shiftKey: true });
    expect(screen.getByRole('menu', { name: 'Actions for Audit session' })).toBeInTheDocument();
    await user.keyboard('{Escape}');
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
    await waitForFocus(trigger);

    fireEvent.keyDown(trigger, { key: 'F10', shiftKey: true });
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
        onSearch={vi.fn()}
        onSelectSession={vi.fn()}
        onNewSession={vi.fn()}
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
    await waitForFocus(screen.getByTitle('Audit session'));
  });

  it('closes the session control menu when pointer focus moves outside it', async () => {
    const user = userEvent.setup();
    render(
      <ProjectsRail
        collapsed={false}
        sessions={[{ id: 'session-1', title: 'Audit session', message_count: 2 }]}
        projects={[]}
        assignments={{}}
        expanded={{}}
        selectedSessionId="session-1"
        sessionIndicators={{}}
        onSearch={vi.fn()}
        onSelectSession={vi.fn()}
        onNewSession={vi.fn()}
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

    const trigger = screen.getByTitle('Audit session');
    fireEvent.keyDown(trigger, { key: 'F10', shiftKey: true });
    expect(screen.getByRole('menu', { name: 'Actions for Audit session' })).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Settings' }));
    expect(screen.queryByRole('menu', { name: 'Actions for Audit session' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Actions for Audit session' })).not.toBeInTheDocument();
  });

  it('filters the flat thread queue from the project scope control', async () => {
    const user = userEvent.setup();
    render(
      <ProjectsRail
        collapsed={false}
        sessions={[
          { id: 'session-1', title: 'Workspace audit', message_count: 2 },
          { id: 'session-2', title: 'General research', message_count: 4 },
        ]}
        projects={[{ id: 'project-1', name: 'Optimus Agent', rootPaths: ['/workspace'] }]}
        assignments={{ 'session-1': 'project-1' }}
        expanded={{}}
        selectedSessionId="session-1"
        sessionIndicators={{ 'session-1': 'working' }}
        onSearch={vi.fn()}
        onSelectSession={vi.fn()}
        onNewSession={vi.fn()}
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

    expect(screen.getByTitle('Workspace audit')).toHaveTextContent('Optimus AgentWorking');
    const unassignedCard = screen.getByTitle('General research');
    expect(unassignedCard).not.toHaveTextContent('No project');
    expect(unassignedCard).not.toHaveTextContent('messages');
    expect(unassignedCard.querySelector('.session-card-meta')).toBeNull();
    expect(unassignedCard.closest('.session-row')).toHaveClass('is-unassigned');

    await user.click(screen.getByRole('button', { name: 'All projects' }));
    await user.click(screen.getByRole('menuitemradio', { name: 'Optimus Agent' }));

    expect(screen.getByTitle('Workspace audit')).toBeInTheDocument();
    expect(screen.queryByTitle('General research')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Optimus Agent' })).toHaveAttribute('aria-expanded', 'false');
  });

  it('dismisses the project scope menu and restores trigger focus', async () => {
    const user = userEvent.setup();
    const onManageProject = vi.fn();
    render(
      <ProjectsRail
        collapsed={false}
        sessions={[]}
        projects={[{ id: 'project-1', name: 'Optimus Agent', rootPaths: ['/workspace'] }]}
        assignments={{}}
        expanded={{}}
        selectedSessionId={null}
        sessionIndicators={{}}
        onSearch={vi.fn()}
        onSelectSession={vi.fn()}
        onNewSession={vi.fn()}
        onAddProject={vi.fn()}
        onManageProject={onManageProject}
        onToggleProject={vi.fn()}
        onTogglePin={vi.fn()}
        onToggleArchive={vi.fn()}
        onAssign={vi.fn()}
        onRename={vi.fn()}
        onDelete={vi.fn()}
        onSettings={vi.fn()}
      />
    );

    const trigger = screen.getByRole('button', { name: 'All projects' });
    await user.click(trigger);
    const firstItem = screen.getByRole('menuitemradio', { name: 'All projects' });
    expect(firstItem).toHaveFocus();
    await user.keyboard('{ArrowDown}');
    expect(screen.getByRole('menuitemradio', { name: 'Optimus Agent' })).toHaveFocus();
    await user.keyboard('{Escape}');
    expect(screen.queryByRole('menu', { name: 'Filter sessions by project' })).not.toBeInTheDocument();
    await waitForFocus(trigger);

    await user.click(trigger);
    await user.click(screen.getByRole('button', { name: 'Settings' }));
    expect(screen.queryByRole('menu', { name: 'Filter sessions by project' })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Settings' })).toHaveFocus();

    await user.click(trigger);
    await user.keyboard('{Enter}');
    expect(screen.queryByRole('menu', { name: 'Filter sessions by project' })).not.toBeInTheDocument();
    await waitForFocus(trigger);

    await user.click(trigger);
    await user.click(screen.getByRole('menuitem', { name: 'Manage sources for Optimus Agent' }));
    expect(onManageProject).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole('menu', { name: 'Filter sessions by project' })).not.toBeInTheDocument();
  });

  it('shows project and worktree metadata only for an explicit assignment', () => {
    render(
      <ProjectsRail
        collapsed={false}
        sessions={[{ id: 'session-1', title: 'Assess Optimus Agent Project State', message_count: 2 }]}
        projects={[{ id: 'optimus-agent', name: 'Optimus Agent', rootPaths: ['/workspace-redesign'], primaryRoot: '/workspace-redesign' }]}
        assignments={{ 'session-1': 'optimus-agent' }}
        expanded={{}}
        selectedSessionId="session-1"
        sessionIndicators={{ 'session-1': 'attention' }}
        onSearch={vi.fn()}
        onSelectSession={vi.fn()}
        onNewSession={vi.fn()}
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

    const card = screen.getByTitle('Assess Optimus Agent Project State');
    expect(card).toHaveTextContent('Optimus Agent');
    expect(card).toHaveTextContent('Attention');
    expect(card).toHaveTextContent('workspace-redesign');
    expect(card).not.toHaveTextContent('messages');
  });

  it('keeps archived threads reachable for recovery outside the footer', async () => {
    const user = userEvent.setup();
    render(
      <ProjectsRail
        collapsed={false}
        sessions={[
          { id: 'active', title: 'Active thread' },
          { id: 'archived', title: 'Archived thread', archived: true },
          { id: 'archived-pinned', title: 'Archived pinned thread', archived: true, pinned: true },
        ]}
        projects={[]}
        assignments={{}}
        expanded={{}}
        selectedSessionId="active"
        sessionIndicators={{}}
        onSearch={vi.fn()}
        onSelectSession={vi.fn()}
        onNewSession={vi.fn()}
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

    expect(screen.getByTitle('Active thread')).toBeInTheDocument();
    expect(screen.queryByTitle('Archived thread')).not.toBeInTheDocument();
    expect(screen.queryByTitle('Archived pinned thread')).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'All projects' }));
    await user.click(screen.getByRole('menuitemradio', { name: 'Archived' }));
    expect(screen.getByRole('button', { name: 'Archived' })).toHaveAttribute('aria-expanded', 'false');
    const archivedThread = screen.getByTitle('Archived thread');
    expect(archivedThread).toBeInTheDocument();
    expect(screen.getByTitle('Archived pinned thread')).toBeInTheDocument();
    fireEvent.contextMenu(archivedThread.closest('.session-row')!);
    expect(screen.getByRole('menuitem', { name: 'Unarchive session' })).toBeInTheDocument();
  });
});

async function waitForFocus(element: HTMLElement) {
  await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
  expect(element).toHaveFocus();
}
