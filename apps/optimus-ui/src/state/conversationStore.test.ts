import { beforeEach, describe, expect, it } from 'vitest';
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
    conversationStore.apply(id, { type: 'tool', name: 'read_file', detail: 'running' });
    conversationStore.apply(id, { type: 'tool', name: 'read_file', detail: 'Read AGENTS.md' });
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
    conversationStore.apply(id, {
      type: 'tool',
      name: 'run_command',
      detail: 'npm test',
    });
    conversationStore.apply(id, { type: 'error', error: 'command failed' });

    expect(conversationStore.get(id).messages.at(-1)?.tools?.[0]?.status).toBe('failed');
  });
});
