import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { DesktopMethod, OptimusTransport } from '../../ipc/contracts';
import { MailPage } from './MailPage';

function fixtureTransport(): OptimusTransport {
  const invoke = vi.fn(async (method: DesktopMethod) => {
    switch (method) {
      case 'gateway_status':
        return {
          status: {
            inbox_pending: 1,
            outbox_total: 1,
            ambiguous_sends: 1,
            note: 'local authority',
          },
        };
      case 'gateway_inbox':
        return {
          messages: [
            {
              id: 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa',
              channel: 'local',
              text: 'hello inbox',
              provider: 'offline',
            },
          ],
        };
      case 'gateway_outbox':
      case 'gateway_ambiguous':
        return {
          messages: [
            {
              message_id: 'bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb',
              outbound: {
                id: 'cccccccc-cccc-cccc-cccc-cccccccccccc',
                in_reply_to: 'bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb',
                channel: 'local',
                text: 'hello outbox',
                status: 'ok',
              },
              terminal_status: 'succeeded',
              ambiguous_send: true,
            },
          ],
        };
      case 'gateway_telegram_status':
        return {
          enabled: false,
          mode: 'mock-or-disabled',
          token_present: false,
          note: 'no public listen',
        };
      case 'gateway_enqueue':
        return { message: { id: 'new', channel: 'local', text: 'x' } };
      case 'gateway_ack_delivery':
        return { acked: true };
      default:
        throw new Error(String(method));
    }
  });
  return {
    kind: 'fixture',
    invoke,
    chat: vi.fn(),
    chatApprovalResolve: vi.fn(),
    windowAction: vi.fn(),
    pickFolder: vi.fn(),
    openPath: vi.fn(),
  } as unknown as OptimusTransport;
}

describe('MailPage', () => {
  it('binds to gateway inbox/outbox without instructional filler copy', async () => {
    const user = userEvent.setup();
    const transport = fixtureTransport();
    render(<MailPage transport={transport} />);
    expect(await screen.findByRole('heading', { name: /Messaging/i })).toBeInTheDocument();
    expect(screen.getAllByText(/hello inbox/i).length).toBeGreaterThan(0);
    expect(screen.queryByText(/Messages are stored|Local gateway|exactly-once|mock\/long-poll/i)).not.toBeInTheDocument();
    expect(screen.queryByLabelText(/Telegram/i)).not.toBeInTheDocument();
    await user.click(screen.getByRole('tab', { name: /outbox/i }));
    expect(screen.getAllByText(/hello outbox/i).length).toBeGreaterThan(0);
    await user.click(screen.getByRole('tab', { name: /Needs review|ambiguous/i }));
    expect(
      await screen.findByRole('button', { name: /Mark as delivered locally/i })
    ).toBeInTheDocument();
  });
});
