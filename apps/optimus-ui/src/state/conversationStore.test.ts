import { beforeEach, describe, expect, it } from 'vitest';
import type { ToolLifecycleEvent } from '../ipc/contracts';
import { frameCoordinator } from '../performance/frameCoordinator';
import { conversationStore } from './conversationStore';

describe('ConversationStore', () => {
  beforeEach(() => frameCoordinator.flushNow());

  it('publishes a burst of deltas once with exact final text', () => {
    const id = `burst-${Date.now()}`;
    conversationStore.load({ id, title: 'Burst', messages: [] });
    conversationStore.begin(id, 'go');
    const before = conversationStore.version(id);

    for (let index = 0; index < 2_000; index += 1) {
      conversationStore.apply(id, { type: 'delta', text: String(index % 10) });
    }

    expect(conversationStore.version(id)).toBe(before);
    frameCoordinator.flushNow();
    const projection = conversationStore.get(id);
    expect(conversationStore.version(id)).toBe(before + 1);
    expect(projection.messages.at(-1)?.content).toHaveLength(2_000);
    expect(projection.messages.at(-1)?.content.endsWith('789')).toBe(true);
  });

  it('does not invalidate the whole session rail for same-state stream frames', () => {
    const id = `indicator-frame-${Date.now()}`;
    conversationStore.load({ id, messages: [] });
    let allNotifications = 0;
    const unsubscribe = conversationStore.subscribeAll(() => {
      allNotifications += 1;
    });

    conversationStore.begin(id, 'stream');
    expect(allNotifications).toBe(1);

    conversationStore.apply(id, { type: 'delta', text: 'partial' });
    frameCoordinator.flushNow();
    conversationStore.apply(id, { type: 'thinking', text: 'reasoning' });
    frameCoordinator.flushNow();
    expect(allNotifications).toBe(1);

    conversationStore.apply(id, { type: 'done' });
    expect(allNotifications).toBe(2);
    unsubscribe();
  });

  it('keeps events and terminal state attached to their owning sessions', () => {
    const first = `first-${Date.now()}`;
    const second = `second-${Date.now()}`;
    conversationStore.load({ id: first, messages: [] });
    conversationStore.load({ id: second, messages: [{ role: 'assistant', content: 'unchanged' }] });
    conversationStore.begin(first, 'work');
    conversationStore.apply(first, { type: 'delta', text: 'partial' });
    conversationStore.apply(first, { type: 'cancelled', error: 'cancelled by user' });

    expect(conversationStore.get(first).status).toBe('cancelled');
    expect(conversationStore.get(first).messages.at(-1)?.content).toBe('partial');
    expect(conversationStore.get(second).messages.at(-1)?.content).toBe('unchanged');
  });

  it('ignores duplicate terminal events after completion', () => {
    const id = `terminal-${Date.now()}`;
    conversationStore.load({ id, messages: [] });
    conversationStore.begin(id, 'work');
    conversationStore.apply(id, { type: 'delta', text: 'result' });
    conversationStore.apply(id, { type: 'done' });
    conversationStore.apply(id, { type: 'error', error: 'late transport error' });
    frameCoordinator.flushNow();
    expect(conversationStore.get(id).status).toBe('completed');
    expect(conversationStore.get(id).statusText).toBe('Completed');
    expect(conversationStore.get(id).messages.at(-1)?.content).toBe('result');
  });

  it('never relabels a disconnected run as completed', () => {
    const id = `disconnected-${Date.now()}`;
    conversationStore.load({ id, messages: [] });
    conversationStore.begin(id, 'work');
    conversationStore.apply(id, { type: 'delta', text: 'partial' });
    conversationStore.markDisconnected(id);
    conversationStore.apply(id, { type: 'done' });

    expect(conversationStore.get(id).status).toBe('disconnected');
    expect(conversationStore.get(id).statusText).toBe(
      'Connection lost · cancellation requested'
    );
    expect(conversationStore.get(id).messages.at(-1)?.content).toBe('partial');
  });

  it('projects truthful per-session rail indicators across run states', () => {
    const id = `indicator-${Date.now()}`;
    conversationStore.load({ id, messages: [] });
    expect(conversationStore.indicator(id)).toBeNull();

    conversationStore.begin(id, 'work');
    expect(conversationStore.indicator(id)).toBe('working');

    conversationStore.apply(id, { type: 'status', text: 'Permission required to continue' });
    expect(conversationStore.indicator(id)).toBe('attention');

    conversationStore.apply(id, { type: 'error', error: 'tool failed' });
    expect(conversationStore.indicator(id)).toBe('error');

    conversationStore.begin(id, 'retry');
    expect(conversationStore.indicator(id)).toBe('working');
    conversationStore.apply(id, { type: 'done' });
    expect(conversationStore.indicator(id)).toBeNull();
  });

  it('treats questions requiring an answer as attention, not active work', () => {
    const id = `question-${Date.now()}`;
    conversationStore.load({ id, messages: [] });
    conversationStore.begin(id, 'work');
    conversationStore.apply(id, { type: 'status', text: 'Awaiting input from the user' });

    expect(conversationStore.get(id).status).toBe('awaiting_approval');
    expect(conversationStore.indicator(id)).toBe('attention');
  });

  it('keeps tool activity on the assistant turn that produced it', () => {
    const id = `tools-${Date.now()}`;
    conversationStore.load({ id, messages: [] });
    conversationStore.begin(id, 'inspect');
    conversationStore.apply(id, toolEvent('call-1', 'started', 'Running'));
    conversationStore.apply(id, toolEvent('call-1', 'succeeded', 'Read AGENTS.md'));
    conversationStore.apply(id, { type: 'done' });

    const firstAssistant = conversationStore
      .get(id)
      .messages.find((message) => message.role === 'assistant');
    expect(firstAssistant?.tools).toEqual([
      expect.objectContaining({
        name: 'read_file',
        detail: 'Read AGENTS.md',
        status: 'completed',
      }),
    ]);

    conversationStore.begin(id, 'continue');
    const assistantTurns = conversationStore
      .get(id)
      .messages.filter((message) => message.role === 'assistant');
    expect(assistantTurns[0]?.tools).toHaveLength(1);
    expect(assistantTurns[1]?.tools).toEqual([]);
  });

  it('marks an unfinished tool as failed when its run fails', () => {
    const id = `tool-failure-${Date.now()}`;
    conversationStore.load({ id, messages: [] });
    conversationStore.begin(id, 'inspect');
    conversationStore.apply(id, toolEvent('call-1', 'started', 'npm test', 'run_command'));
    conversationStore.apply(id, { type: 'error', error: 'command failed' });

    expect(conversationStore.get(id).messages.at(-1)?.tools?.[0]?.status).toBe('failed');
  });

  it('keeps an exact-action tool awaiting approval when the stream closes', () => {
    const id = `tool-approval-${Date.now()}`;
    conversationStore.load({ id, messages: [] });
    conversationStore.begin(id, 'update the file');
    conversationStore.apply(id, toolEvent('call-1', 'started', 'Running', 'write_file'));
    conversationStore.apply(
      id,
      toolEvent('call-1', 'approval_required', 'Write src/app.ts (12 bytes)', 'write_file')
    );
    conversationStore.apply(id, { type: 'error', error: 'approval required' });

    expect(conversationStore.get(id).status).toBe('awaiting_approval');
    expect(conversationStore.get(id).messages.at(-1)?.tools?.[0]).toEqual(
      expect.objectContaining({
        status: 'awaiting_approval',
        detail: 'Write src/app.ts (12 bytes)',
      })
    );
  });

  it('replays durable tool receipts on reload and ignores reconnect duplicates', () => {
    const id = `tool-reload-${Date.now()}`;
    const started = toolEvent('call-1', 'started', 'Reading', 'read_file');
    const succeeded = toolEvent('call-1', 'succeeded', 'Read README.md', 'read_file');
    conversationStore.load({
      id,
      run_status: 'succeeded',
      messages: [
        { role: 'user', content: 'inspect' },
        { role: 'assistant', content: 'Done.', tool_events: [started, succeeded] },
      ],
    });

    expect(conversationStore.get(id).status).toBe('idle');
    expect(conversationStore.get(id).messages.at(-1)?.tools).toEqual([
      expect.objectContaining({
        callId: 'call-1',
        detail: 'Read README.md',
        status: 'completed',
      }),
    ]);

    conversationStore.apply(id, succeeded);
    expect(conversationStore.get(id).messages.at(-1)?.tools).toHaveLength(1);
  });

  it('restores exact approval state even though the interrupted turn settled as failed', () => {
    const id = `approval-reload-${Date.now()}`;
    conversationStore.load({
      id,
      run_status: 'failed',
      messages: [
        { role: 'user', content: 'update the file' },
        {
          role: 'assistant',
          content: '',
          tool_events: [
            toolEvent('call-1', 'started', 'Running', 'write_file'),
            toolEvent(
              'call-1',
              'approval_required',
              'Write src/app.ts (12 bytes)',
              'write_file'
            ),
          ],
        },
      ],
    });

    expect(conversationStore.get(id).status).toBe('awaiting_approval');
    expect(conversationStore.get(id).messages.at(-1)?.tools?.[0]?.status).toBe(
      'awaiting_approval'
    );
  });

  it('retains the exact approval binding through durable replay and later lifecycle updates', () => {
    const id = `approval-binding-reload-${Date.now()}`;
    const approval = {
      run_id: 'run-1',
      call_id: 'call-1',
      tool_id: 'write_file',
      job_id: 'job-approval-1',
      node_id: 'node-7',
      node_index: 7,
      effect_sha256: 'c'.repeat(64),
      summary: 'Write src/app.ts',
    };
    conversationStore.load({
      id,
      run_status: 'failed',
      messages: [
        { role: 'user', content: 'update the file' },
        {
          role: 'assistant',
          content: '',
          tool_events: [
            toolEvent('call-1', 'started', 'Preparing write', 'write_file'),
            { ...toolEvent('call-1', 'approval_required', 'Write src/app.ts', 'write_file'), approval },
          ],
        },
      ],
    });

    expect(conversationStore.get(id).messages.at(-1)?.tools?.[0]).toEqual(
      expect.objectContaining({
        status: 'awaiting_approval',
        approval,
      })
    );

    conversationStore.apply(id, toolEvent('call-1', 'succeeded', 'Write completed', 'write_file'));
    expect(conversationStore.get(id).messages.at(-1)?.tools?.[0]).toEqual(
      expect.objectContaining({
        status: 'completed',
        approval,
      })
    );
  });

  it('restores a failed run even when its last tool completed successfully', () => {
    const id = `failed-after-tool-${Date.now()}`;
    conversationStore.load({
      id,
      run_status: 'failed',
      messages: [
        { role: 'user', content: 'inspect then answer' },
        {
          role: 'assistant',
          content: '',
          tool_events: [
            toolEvent('call-1', 'started', 'Reading', 'read_file'),
            toolEvent('call-1', 'succeeded', 'Read README.md', 'read_file'),
          ],
        },
      ],
    });

    expect(conversationStore.get(id).status).toBe('failed');
    expect(conversationStore.get(id).statusText).toBe('Run failed');
  });

  it('does not let an older approval override a newer completed turn', () => {
    const id = `stale-approval-${Date.now()}`;
    conversationStore.load({
      id,
      run_status: 'succeeded',
      messages: [
        { role: 'user', content: 'update the file' },
        {
          role: 'assistant',
          content: '',
          tool_events: [
            toolEvent('call-1', 'started', 'Running', 'write_file'),
            toolEvent('call-1', 'approval_required', 'Write src/app.ts (12 bytes)', 'write_file'),
          ],
        },
        { role: 'user', content: 'leave it unchanged' },
        { role: 'assistant', content: 'Left unchanged.' },
      ],
    });

    expect(conversationStore.get(id).status).toBe('idle');
    expect(conversationStore.get(id).statusText).toBe('');
    expect(conversationStore.get(id).messages[1]?.tools?.[0]?.status).toBe(
      'awaiting_approval'
    );
  });

  it('keeps thinking deltas out of assistant answer text', () => {
    conversationStore.begin('s-think', 'hi');
    conversationStore.apply('s-think', {
      type: 'thinking',
      text: 'Consider options…\n',
    });
    conversationStore.apply('s-think', { type: 'delta', text: 'Final answer.' });
    frameCoordinator.flushNow();
    conversationStore.apply('s-think', { type: 'done' });
    const projection = conversationStore.get('s-think');
    const assistant = projection.messages.find((m) => m.role === 'assistant');
    expect(assistant?.thinking).toContain('Consider options');
    expect(assistant?.content).toBe('Final answer.');
    expect(assistant?.content).not.toContain('Consider options');
  });

  it('uses call identity and explicit phase for overlapping and duplicate tool events', () => {
    const id = `tool-identity-${Date.now()}`;
    conversationStore.load({ id, messages: [] });
    conversationStore.begin(id, 'inspect twice');

    conversationStore.apply(id, toolEvent('call-1', 'started', 'First read'));
    conversationStore.apply(id, toolEvent('call-2', 'started', 'Second read'));
    conversationStore.apply(id, toolEvent('call-1', 'succeeded', 'No failure despite this word'));
    conversationStore.apply(id, toolEvent('call-1', 'succeeded', 'No failure despite this word'));

    expect(conversationStore.get(id).messages.at(-1)?.tools).toEqual([
      expect.objectContaining({ callId: 'call-1', status: 'completed' }),
      expect.objectContaining({ callId: 'call-2', status: 'running' }),
    ]);
  });
});

function toolEvent(
  callId: string,
  phase: 'started' | 'approval_required' | 'succeeded',
  summary: string,
  toolId = 'read_file'
): ToolLifecycleEvent {
  return {
    type: 'tool',
    schema_version: 1,
    event_id: `run-1:${callId}:${phase}`,
    run_id: 'run-1',
    call_id: callId,
    tool_id: toolId,
    phase,
    summary,
    duration_ms: phase === 'started' ? undefined : 12,
  };
}
