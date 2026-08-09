import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it } from 'vitest';
import { OptimusApp } from './OptimusApp';

describe('OptimusApp fixture contract', () => {
  beforeEach(() => localStorage.clear());

  it('renders the dense workbench and honest capability boundaries', async () => {
    const { container } = render(<OptimusApp />);
    expect(await screen.findByRole('complementary', { name: 'Projects and sessions' })).toBeInTheDocument();
    // First-run catalog is empty — no hard-coded project seed (#42).
    expect(screen.queryByRole('button', { name: 'Project Optimus Agent' })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Settings' })).toHaveTextContent('Settings');
    expect(screen.queryByRole('button', { name: 'Mail' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Capabilities' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Resources' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Show archived' })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'All projects' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Inbox' })).not.toBeInTheDocument();
    expect(screen.getByRole('log', { name: 'Conversation' })).toBeInTheDocument();
    // Chat-first: evidence workspace starts closed until Browser/Files/Artifacts is opened.
    expect(screen.queryByRole('complementary', { name: 'Evidence workspace' })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Workspace' })).toBeInTheDocument();
    expect(screen.getByLabelText('Message Optimus')).toBeInTheDocument();
    expect(screen.queryByRole('contentinfo', { name: 'System status' })).not.toBeInTheDocument();
    const sendIcon = screen.getByRole('button', { name: 'Send message' }).querySelector('.optimus-icon');
    expect(sendIcon).toHaveAttribute('data-icon', 'send');

    const windowControls = container.querySelector('.window-controls');
    expect(windowControls).toBeInTheDocument();
    for (const action of ['minimize', 'maximize', 'close']) {
      const button = within(windowControls as HTMLElement).getByRole('button', {
        name: action[0].toUpperCase() + action.slice(1),
      });
      expect(button.querySelector(`[data-window-icon="${action}"]`)).toBeInTheDocument();
      expect(button.querySelector('span.window-control-icon')).toBeInTheDocument();
    }
  });

  it('sends Auto chat and the fixture resolves it offline on first run', async () => {
    const user = userEvent.setup();
    render(<OptimusApp />);
    const composer = await screen.findByLabelText('Message Optimus');
    await user.type(composer, 'hello offline');
    await user.click(screen.getByRole('button', { name: 'Send message' }));
    expect(screen.queryByRole('dialog', { name: 'Project sources' })).not.toBeInTheDocument();
    expect(
      await screen.findByText(
        /working from the offline fixture transport/i,
        {},
        { timeout: 5000 }
      )
    ).toBeInTheDocument();
  });

  it('surfaces a rejected chat start with the real error, not Connection lost', async () => {
    const user = userEvent.setup();
    render(<OptimusApp />);
    const composer = await screen.findByLabelText('Message Optimus');
    await user.type(composer, 'fixture chat error');
    await user.click(screen.getByRole('button', { name: 'Send message' }));
    // The configuration error ("No DeepSeek API key…") must reach the user;
    // the pre-fix path flattened every non-abort rejection into the
    // misleading "Connection lost · cancellation requested".
    const matches = await screen.findAllByText(/No DeepSeek API key/i);
    expect(matches.length).toBeGreaterThan(0);
    expect(screen.queryByText(/Connection lost/i)).not.toBeInTheDocument();
  });

  it('streams an approval continuation live instead of sticking on Approving…', async () => {
    const user = userEvent.setup();
    render(<OptimusApp />);
    await user.click(
      await screen.findByRole('button', { name: /Assess Optimus Agent Project State/ })
    );

    // The parked command renders an approval card.
    const approve = await screen.findByRole('button', { name: 'Approve and continue' });
    await user.click(approve);

    // The button flips to the in-flight label while the effect settles…
    expect(await screen.findByRole('button', { name: 'Approving…' })).toBeInTheDocument();

    // …and the resumed turn's answer streams into the transcript without any
    // further clicks. The pre-fix desktop resolve ran the whole continuation
    // as one invisible blocking call, so the button stayed on "Approving…"
    // with no feedback and no way to stop it.
    expect(
      await screen.findByText(/The approved command ran/i, {}, { timeout: 5000 })
    ).toBeInTheDocument();
    // The settlement event closes the card: the decision is a stream, so the
    // in-flight label is replaced by the settled row, never stuck forever.
    await waitFor(() =>
      expect(screen.queryByRole('button', { name: 'Approving…' })).not.toBeInTheDocument()
    );
    expect(screen.queryByRole('button', { name: 'Approve and continue' })).not.toBeInTheDocument();
  });

  it('keeps the topbar focused on home, workspace expansion, terminal, and the right panel', async () => {
    const user = userEvent.setup();
    const { container } = render(<OptimusApp />);

    await screen.findByRole('complementary', { name: 'Projects and sessions' });
    const topbar = container.querySelector('.topbar');
    expect(topbar).toBeInTheDocument();
    expect(within(topbar as HTMLElement).queryByRole('button', { name: 'Back' })).not.toBeInTheDocument();
    expect(within(topbar as HTMLElement).queryByRole('button', { name: 'Forward' })).not.toBeInTheDocument();
    expect(within(topbar as HTMLElement).getByRole('button', { name: 'Optimus' })).toBeInTheDocument();
    expect(within(topbar as HTMLElement).getByRole('button', { name: 'Maximize workspace' })).toBeInTheDocument();
    expect(within(topbar as HTMLElement).getByRole('button', { name: 'Terminal' })).toBeInTheDocument();
    expect(within(topbar as HTMLElement).getByRole('button', { name: 'Workspace' })).toBeInTheDocument();
    for (const removed of ['Tasks', 'Browser', 'Files', 'Artifacts', 'Use light theme']) {
      expect(within(topbar as HTMLElement).queryByRole('button', { name: removed })).not.toBeInTheDocument();
    }

    await user.click(within(topbar as HTMLElement).getByRole('button', { name: 'Optimus' }));
    expect(screen.getByRole('log', { name: 'Conversation' })).toBeInTheDocument();

    const projectRail = screen.getByRole('complementary', { name: 'Projects and sessions' });
    const closeRail = within(topbar as HTMLElement).getByRole('button', { name: 'Close project rail' });
    expect(projectRail).not.toHaveClass('is-collapsed');
    await user.click(closeRail);
    expect(projectRail).toHaveClass('is-collapsed');
    expect(within(topbar as HTMLElement).getByRole('button', { name: 'Open project rail' })).toHaveAttribute(
      'aria-pressed',
      'false'
    );
    await user.click(within(topbar as HTMLElement).getByRole('button', { name: 'Open project rail' }));
    expect(projectRail).not.toHaveClass('is-collapsed');

    await user.click(within(topbar as HTMLElement).getByRole('button', { name: 'Maximize workspace' }));
    expect(container.querySelector('.surface-row')).toHaveClass('is-workspace-maximized');
    expect(await within(topbar as HTMLElement).findByRole('button', { name: 'Restore workspace' })).toBeInTheDocument();
    await user.click(within(topbar as HTMLElement).getByRole('button', { name: 'Restore workspace' }));
    expect(container.querySelector('.surface-row')).not.toHaveClass('is-workspace-maximized');
  });

  it('returns to the work surface when the terminal is toggled closed', async () => {
    const user = userEvent.setup();
    const { container } = render(<OptimusApp />);
    const app = container.querySelector('.optimus-app');
    const terminal = await screen.findByRole('button', { name: 'Terminal' });

    await user.click(terminal);
    expect(app).toHaveAttribute('data-compact-surface', 'execution');
    expect(screen.getByRole('complementary', { name: 'Execution dock' })).toHaveClass('is-open');

    await user.click(terminal);
    expect(app).toHaveAttribute('data-compact-surface', 'work');
    expect(screen.getByRole('complementary', { name: 'Execution dock' })).not.toHaveClass('is-open');
    expect(screen.getByRole('region', { name: 'Agent work surface' })).toBeInTheDocument();
  });

  it('keeps every evidence surface reachable on a wide desktop layout', async () => {
    // Regression: workspaceTab was only settable from the compact switcher, which
    // is display:none above 899px, so a desktop window could reach Browser only.
    const user = userEvent.setup();
    render(<OptimusApp />);
    await screen.findByRole('complementary', { name: 'Projects and sessions' });
    await user.click(screen.getByRole('button', { name: 'Workspace' }));

    const tabs = screen.getByRole('tablist', { name: 'Evidence surface' });
    expect(within(tabs).getByRole('tab', { name: 'Browser' })).toHaveAttribute(
      'aria-selected',
      'true'
    );

    await user.click(within(tabs).getByRole('tab', { name: 'Files' }));
    expect(within(tabs).getByRole('tab', { name: 'Files' })).toHaveAttribute(
      'aria-selected',
      'true'
    );
    expect(document.getElementById('workspace-panel-files')).not.toHaveAttribute('hidden');

    await user.click(within(tabs).getByRole('tab', { name: 'Artifacts' }));
    expect(document.getElementById('workspace-panel-artifacts')).not.toHaveAttribute('hidden');
    expect(document.getElementById('workspace-panel-files')).toHaveAttribute('hidden');
    // Await the surface's own async load so it cannot resolve after teardown.
    expect(await screen.findByRole('region', { name: 'Artifacts' })).toBeInTheDocument();
  });

  it('keeps runtime capabilities reachable from the command palette', async () => {
    const user = userEvent.setup();
    render(<OptimusApp />);
    await screen.findByRole('complementary', { name: 'Projects and sessions' });

    await user.keyboard('{Control>}k{/Control}');
    await screen.findByRole('dialog', { name: 'Command palette' });
    // An option, not a button: the palette is a listbox now, which is what lets
    // it be driven from the keyboard at all (ADR-0050).
    await user.click(await screen.findByRole('option', { name: /capabilities/i }));
    expect(await screen.findByRole('main', { name: 'Capabilities' })).toBeInTheDocument();
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

  it('dismisses authorize dialog with Escape and clears the authorize banner', async () => {
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
    await user.type(composer, 'escape me');
    await user.click(screen.getByRole('button', { name: 'Send message' }));
    expect(await screen.findByRole('dialog', { name: 'Project sources' })).toBeInTheDocument();

    await user.keyboard('{Escape}');
    expect(screen.queryByRole('dialog', { name: 'Project sources' })).not.toBeInTheDocument();
    expect(
      screen.queryByText('Authorize this project folder before running its session.')
    ).not.toBeInTheDocument();
  });

  it('continues without project and unassigns the session', async () => {
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
    await user.clear(composer);
    await user.type(composer, 'run unbound');
    await user.click(screen.getByRole('button', { name: 'Send message' }));
    expect(await screen.findByRole('dialog', { name: 'Project sources' })).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Continue without project' }));
    expect(screen.queryByRole('dialog', { name: 'Project sources' })).not.toBeInTheDocument();
    expect(JSON.parse(localStorage.getItem('optimus.ui.sessionProjects') || '{}')).not.toHaveProperty(
      'fixture-assess'
    );
  });

  it('contains Settings focus, closes from external focus, and restores its trigger', async () => {
    const user = userEvent.setup();
    render(<OptimusApp />);
    const settings = await screen.findByRole('button', { name: 'Settings' });
    await user.click(settings);
    const dialog = await screen.findByRole('dialog', { name: 'Settings' });
    expect(document.querySelector('.topbar')).toHaveProperty('inert', true);
    expect(dialog.closest('[inert]')).toBeNull();
    await waitFor(() => expect(dialog).toContainElement(document.activeElement as HTMLElement));

    document.body.focus();
    await user.keyboard('{Escape}');
    expect(screen.queryByRole('dialog', { name: 'Settings' })).not.toBeInTheDocument();
    expect(settings).toHaveFocus();
    expect(document.querySelector('.topbar')).toHaveProperty('inert', false);
  });

  it('exposes bounded keyboard controls for every workbench separator', async () => {
    const user = userEvent.setup();
    render(<OptimusApp />);
    const rail = await screen.findByRole('separator', { name: 'Resize project rail' });
    expect(rail).toHaveAttribute('tabindex', '0');
    expect(rail).toHaveAttribute('aria-valuemin', '200');
    const railStart = Number(rail.getAttribute('aria-valuenow'));
    rail.focus();
    await user.keyboard('{ArrowRight}');
    expect(Number(rail.getAttribute('aria-valuenow'))).toBe(railStart + 10);

    await user.click(screen.getByRole('button', { name: 'Workspace' }));
    const workspace = screen.getByRole('separator', { name: 'Resize evidence workspace' });
    const workspaceStart = Number(workspace.getAttribute('aria-valuenow'));
    workspace.focus();
    await user.keyboard('{ArrowLeft}');
    expect(Number(workspace.getAttribute('aria-valuenow'))).toBe(workspaceStart + 10);

    await user.click(screen.getByRole('button', { name: 'Workspace' }));
    expect(screen.queryByRole('separator', { name: 'Resize evidence workspace' })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Workspace' })).toHaveAttribute('aria-pressed', 'false');

    await user.click(screen.getByRole('button', { name: 'Terminal' }));
    const execution = screen.getByRole('separator', { name: 'Resize execution dock' });
    expect(execution).toHaveAttribute('aria-orientation', 'horizontal');
    execution.focus();
    await user.keyboard('{End}');
    expect(execution).toHaveAttribute('aria-valuenow', '520');
  });

});
