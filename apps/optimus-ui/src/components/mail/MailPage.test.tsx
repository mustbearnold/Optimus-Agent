import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { MailPage } from './MailPage';

describe('MailPage', () => {
  it('opens Optimus update mail and marks the selected message as read', async () => {
    const user = userEvent.setup();
    render(
      <MailPage
        projects={[{
          id: 'optimus-agent',
          name: 'Optimus Agent',
          rootPaths: ['/workspace/optimus-agent'],
        }]}
        sessions={[{
          id: 'session-1',
          title: 'Review current changes',
          message_count: 4,
        }]}
        assignments={{ 'session-1': 'optimus-agent' }}
        activeRunSessionId={null}
      />
    );

    expect(screen.getByRole('main', { name: 'Mail' })).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Optimus Agent workspace summary' })).toBeInTheDocument();
    expect(await screen.findByText('1 unread')).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: /Welcome to Optimus Mail/ }));

    expect(screen.getByRole('heading', { name: 'Welcome to Optimus Mail' })).toBeInTheDocument();
    expect(screen.getByText('All caught up')).toBeInTheDocument();
    expect(screen.getByText(/External email delivery and account sync are not implemented/)).toBeInTheDocument();
  });
});
