import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { createOptimusClient } from '../../ipc/client';
import { WorkspacePane } from './WorkspacePane';

describe('WorkspacePane tabs (06-preview-browser.spec.js parity)', () => {
  it('switches surfaces from the tablist with proper ARIA wiring', () => {
    const onSelectTab = vi.fn();
    render(
      <WorkspacePane
        tab="browser"
        client={createOptimusClient(null)}
        suspended={false}
        onAddToPrompt={vi.fn()}
        onSelectTab={onSelectTab}
      />
    );

    const tabs = screen.getByRole('tablist', { name: 'Evidence surface' });
    expect(tabs).toBeInTheDocument();
    const browserTab = screen.getByRole('tab', { name: 'Browser' });
    expect(browserTab).toHaveAttribute('aria-selected', 'true');
    expect(browserTab).toHaveAttribute('aria-controls', 'workspace-panel-browser');

    fireEvent.click(screen.getByRole('tab', { name: 'Files' }));
    expect(onSelectTab).toHaveBeenCalledWith('files');
  });

  it('only keeps the active surface mounted (browser bootstraps lazily)', () => {
    render(
      <WorkspacePane
        tab="files"
        client={createOptimusClient(null)}
        suspended={false}
        onAddToPrompt={vi.fn()}
        onSelectTab={vi.fn()}
      />
    );

    expect(screen.getByRole('tabpanel', { name: 'Files' })).toBeInTheDocument();
    // The inactive panels are hidden, not removed — the ARIA tab contract.
    // (Name matching skips hidden elements; match by id over the hidden set.)
    const browserPanel = screen
      .getAllByRole('tabpanel', { hidden: true })
      .find((panel) => panel.id === 'workspace-panel-browser');
    expect(browserPanel).toBeInTheDocument();
    expect(browserPanel).toHaveAttribute('hidden');
  });
});
