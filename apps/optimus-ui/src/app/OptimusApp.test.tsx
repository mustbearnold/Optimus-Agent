import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it } from 'vitest';
import { OptimusApp } from './OptimusApp';

describe('OptimusApp fixture contract', () => {
  beforeEach(() => localStorage.clear());

  it('renders the dense workbench and honest capability boundaries', async () => {
    const { container } = render(<OptimusApp />);
    expect(await screen.findByRole('complementary', { name: 'Projects and sessions' })).toBeInTheDocument();
    expect(await screen.findByRole('button', { name: 'Project Optimus Agent' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Mail' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Inbox' })).not.toBeInTheDocument();
    expect(screen.getByRole('log', { name: 'Conversation' })).toBeInTheDocument();
    expect(screen.getByRole('complementary', { name: 'Evidence workspace' })).toBeInTheDocument();
    expect(screen.getByLabelText('Message Optimus')).toBeInTheDocument();

    const windowControls = container.querySelector('.window-controls');
    expect(windowControls).toBeInTheDocument();
    for (const action of ['minimize', 'maximize', 'close']) {
      const button = within(windowControls as HTMLElement).getByRole('button', {
        name: action[0].toUpperCase() + action.slice(1),
      });
      expect(button.querySelector(`[data-window-icon="${action}"]`)).toBeInTheDocument();
      expect(button.querySelector('svg.reicon')).not.toBeInTheDocument();
    }
  });

  it('moves backward and forward through visited areas with tailed arrow controls', async () => {
    const user = userEvent.setup();
    const { container } = render(<OptimusApp />);

    await screen.findByRole('complementary', { name: 'Projects and sessions' });
    const topbar = container.querySelector('.topbar');
    expect(topbar).toBeInTheDocument();
    const primaryNavigation = screen.getByRole('navigation', { name: 'Primary' });
    const back = within(topbar as HTMLElement).getByRole('button', { name: 'Back' });
    const forward = within(topbar as HTMLElement).getByRole('button', { name: 'Forward' });
    expect(back).toBeDisabled();
    expect(forward).toBeDisabled();
    expect(back.querySelector('svg.reicon')).toBeInTheDocument();
    expect(forward.querySelector('svg.reicon')).toBeInTheDocument();

    await user.click(within(primaryNavigation).getByRole('button', { name: 'Mail' }));
    expect(await screen.findByRole('main', { name: 'Mail' })).toBeInTheDocument();
    await user.click(within(primaryNavigation).getByRole('button', { name: 'Capabilities' }));
    expect(await screen.findByRole('main', { name: 'Capabilities' })).toBeInTheDocument();
    await user.click(within(primaryNavigation).getByRole('button', { name: 'Artifacts' }));
    expect(await screen.findByRole('region', { name: 'Artifacts' })).toBeInTheDocument();

    await user.click(back);
    expect(await screen.findByRole('main', { name: 'Capabilities' })).toBeInTheDocument();
    await user.click(forward);
    expect(await screen.findByRole('region', { name: 'Artifacts' })).toBeInTheDocument();

    await user.click(back);
    await user.click(screen.getByRole('button', { name: 'Optimus' }));
    expect(screen.getByRole('log', { name: 'Conversation' })).toBeInTheDocument();
    expect(forward).toBeDisabled();
  });

  it('blocks an assigned session instead of falling back to an unauthorized workspace', async () => {
    localStorage.setItem(
      'optimus.ui.projects',
      JSON.stringify({
        version: 2,
        projects: [
          {
            id: 'private-project',
            name: 'Private Project',
            rootPaths: ['/private/project'],
            primaryRoot: '/private/project',
          },
        ],
      })
    );
    localStorage.setItem(
      'optimus.ui.sessionProjects',
      JSON.stringify({ 'fixture-assess': 'private-project' })
    );
    const user = userEvent.setup();
    render(<OptimusApp />);

    const composer = await screen.findByLabelText('Message Optimus');
    await user.type(composer, 'Do not run this elsewhere');
    await user.click(screen.getByRole('button', { name: 'Send message' }));

    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Authorize this project folder before running its session.'
    );
    expect(screen.getByRole('dialog', { name: 'Project sources' })).toBeInTheDocument();
    expect(composer).toHaveValue('Do not run this elsewhere');
  });
});
