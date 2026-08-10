import { describe, expect, it, vi } from 'vitest';
import userEvent from '@testing-library/user-event';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import {
  formatAuthorizeError,
  normalizePathKey,
  ProjectSourcesDialog,
} from './ProjectSourcesDialog';
import type { Project } from '../../ipc/contracts';

function fixtureProject(overrides: Partial<Project> = {}): Project {
  return {
    id: 'optimus-agent',
    name: 'Optimus Agent',
    rootPaths: ['/projects/optimus-agent'],
    primaryRoot: '/projects/optimus-agent',
    pinned: true,
    ...overrides,
  };
}

describe('ProjectSourcesDialog authorization gates', () => {
  it('blocks save until unauthorized roots are re-selected natively', async () => {
    const onSave = vi.fn();
    const onPickSource = vi.fn().mockResolvedValue({
      ok: true,
      path: '/projects/optimus-agent',
      grantToken: 'grant-1',
    });

    render(
      <ProjectSourcesDialog
        project={fixtureProject()}
        authorizedRootPaths={[]}
        onPickSource={onPickSource}
        onSave={onSave}
        onClose={() => {}}
      />
    );

    const save = screen.getByRole('button', { name: /re-select folders to authorize/i });
    expect(save).toBeDisabled();
    expect(screen.getByRole('status')).toHaveTextContent(/needs native folder re-selection/i);

    fireEvent.click(screen.getByRole('button', { name: 'Re-select folder' }));
    await waitFor(() => expect(onPickSource).toHaveBeenCalled());

    const readySave = await screen.findByRole('button', { name: 'Save & authorize' });
    expect(readySave).not.toBeDisabled();
    fireEvent.click(readySave);
    await waitFor(() =>
      expect(onSave).toHaveBeenCalledWith(
        expect.objectContaining({
          id: 'optimus-agent',
          rootPaths: ['/projects/optimus-agent'],
        }),
        ['grant-1']
      )
    );
  });

  it('allows rename-only save when roots are already authorized', () => {
    const onSave = vi.fn();
    render(
      <ProjectSourcesDialog
        project={fixtureProject({ name: 'Renamed' })}
        authorizedRootPaths={['/projects/optimus-agent']}
        onPickSource={vi.fn()}
        onSave={onSave}
        onClose={() => {}}
      />
    );
    const save = screen.getByRole('button', { name: /save & authorize/i });
    expect(save).not.toBeDisabled();
    expect(screen.getByRole('status')).toHaveTextContent(/^Authorized$/);
  });

  it('formats opaque request failures into actionable copy', () => {
    expect(formatAuthorizeError(new Error('request failed'))).toMatch(/system picker/i);
    expect(
      formatAuthorizeError(
        new Error('project root requires a current native folder selection: /tmp/x')
      )
    ).toMatch(/Re-select folder/i);
    expect(normalizePathKey('/tmp/demo/')).toBe('/tmp/demo');
  });

  it('closes on Escape and exposes continue-without-project when allowed', async () => {
    const onClose = vi.fn();
    const onContinue = vi.fn();
    render(
      <ProjectSourcesDialog
        project={fixtureProject()}
        authorizedRootPaths={[]}
        allowContinueWithoutProject
        onPickSource={vi.fn()}
        onSave={vi.fn()}
        onContinueWithoutProject={onContinue}
        onClose={onClose}
      />
    );

    expect(screen.getByRole('button', { name: 'Continue without project' })).toBeInTheDocument();
    // Radix handles Escape on the dialog content (modal scope) — the
    // hand-rolled window-level listener is gone (ADR-0050).
    fireEvent.keyDown(screen.getByRole('dialog', { name: 'Project sources' }), { key: 'Escape' });
    expect(onClose).toHaveBeenCalled();
  });

  it('contains focus inside the Radix modal scope and wraps on Tab', async () => {
    const user = userEvent.setup();
    render(
      <ProjectSourcesDialog
        project={fixtureProject()}
        authorizedRootPaths={[]}
        onPickSource={vi.fn()}
        onSave={vi.fn()}
        onClose={() => {}}
      />
    );

    const dialog = await screen.findByRole('dialog', { name: 'Project sources' });
    // Radix moves focus into the dialog on open (focus scope).
    await waitFor(() => expect(dialog).toContainElement(document.activeElement as HTMLElement));

    // Tab cannot leave the dialog: from the last control it wraps to the
    // first (Radix focus scope — same contract as the Settings dialog).
    const controls = screen.getAllByRole('button');
    controls[controls.length - 1].focus();
    await user.keyboard('{Tab}');
    await waitFor(() => expect(dialog).toContainElement(document.activeElement as HTMLElement));
  });
});
