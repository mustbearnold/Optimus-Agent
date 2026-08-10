import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, beforeEach } from 'vitest';
import { OptimusApp } from './OptimusApp';
import { conversationStore } from '../state/conversationStore';
import { getTransport, initTransport } from '../ipc';

describe('session consent surface (spec-014 R7 / R12)', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it('offers "Always allow <class>" only on command approvals and grants before resolve', async () => {
    const user = userEvent.setup();
    render(<OptimusApp />);
    // Open the fixture session that carries the parked command.
    await user.click(
      await screen.findByRole('button', { name: /Assess Optimus Agent Project State/ })
    );
    // The parked command renders an approval card.
    await screen.findByRole('button', { name: 'Approve and continue' });
    // The fixture approval is a command (bun test → project_execute) so the
    // consent checkbox must be offered.
    const checkbox = await screen.findByRole('checkbox', {
      name: /always allow/i,
    });
    await user.click(checkbox);
    await user.click(screen.getByRole('button', { name: 'Approve and continue' }));
    // Grant-before-resolve: the fixture transport records the consent when
    // the approval resolves; the card must leave the pending state.
    await waitFor(() => {
      expect(
        screen.queryByRole('checkbox', { name: /always allow/i })
      ).not.toBeInTheDocument();
    });
  });

  it('keeps the once-per-session profile banner dismissed on the projection the component renders', () => {
    const sessionId = 'fixture-banner-session';
    conversationStore.load({
      id: sessionId,
      title: 'banner session',
      run_status: 'awaiting_approval',
      messages: [
        {
          id: `${sessionId}:m1`,
          role: 'assistant',
          content: '',
          status: 'working',
          tool_events: [
            { event_id: 'e1', call_id: 'c1', phase: 'approval_required', summary: 'a' },
            { event_id: 'e2', call_id: 'c2', phase: 'approval_required', summary: 'b' },
            { event_id: 'e3', call_id: 'c3', phase: 'approval_required', summary: 'c' },
          ],
        },
      ],
    } as never);
    // The component renders conversationStore.get(sessionId).suggestProfileBanner
    // (OptimusApp.tsx). Assert the projection field, not the store method.
    expect(conversationStore.get(sessionId).suggestProfileBanner).toBe(true);
    conversationStore.dismissProfileBanner(sessionId);
    // Dismissal must flip the projection immediately. Without the fix, the
    // projection stayed true and the banner reappeared on the next render.
    expect(conversationStore.get(sessionId).suggestProfileBanner).toBe(false);
    // A rebuild (fresh load with new approval events) must keep it false.
    // The streak crosses 3 again; the dismissed guard must hold.
    conversationStore.load({
      id: sessionId,
      title: 'banner session',
      run_status: 'awaiting_approval',
      messages: [
        {
          id: `${sessionId}:m2`,
          role: 'assistant',
          content: '',
          status: 'working',
          tool_events: [
            { event_id: 'e4', call_id: 'c4', phase: 'approval_required', summary: 'd' },
            { event_id: 'e5', call_id: 'c5', phase: 'approval_required', summary: 'e' },
            { event_id: 'e6', call_id: 'c6', phase: 'approval_required', summary: 'f' },
          ],
        },
      ],
    } as never);
    expect(conversationStore.get(sessionId).suggestProfileBanner).toBe(false);
  });

  it('revokes session grants from settings', async () => {
    const user = userEvent.setup();
    render(<OptimusApp />);
    // Open the fixture session so the settings dialog carries a session id.
    await user.click(
      await screen.findByRole('button', { name: /Assess Optimus Agent Project State/ })
    );
    // Seed a live grant through the same fixture transport the app uses.
    await initTransport();
    await getTransport()!.invoke('session_consent_grant', {
      session_id: 'fixture-assess',
      command_class: 'project_execute',
    });
    // Open settings, then the Terminal & execution section that hosts the
    // DeveloperAccessPanel.
    await user.click(screen.getByRole('button', { name: 'Settings' }));
    await user.click(screen.getByRole('button', { name: 'Terminal & execution' }));
    // The seeded grant must appear in the session grants list.
    await screen.findByText('project_execute');
    await user.click(screen.getByRole('button', { name: 'Revoke session grants' }));
    await waitFor(() => {
      expect(screen.getByText('Revoked 1 session grant.')).toBeInTheDocument();
    });
  });
});
