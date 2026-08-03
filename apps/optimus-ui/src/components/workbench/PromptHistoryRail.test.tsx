import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { PromptHistoryRail } from './PromptHistoryRail';

describe('PromptHistoryRail', () => {
  it('indexes only user messages and navigates to the selected prompt', async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    render(
      <PromptHistoryRail
        messages={[
          { id: 'u1', role: 'user', content: 'Inspect the shell' },
          { id: 'a1', role: 'assistant', content: 'The shell is ready.' },
          { id: 'u2', role: 'user', content: 'Run the visual smoke test' },
        ]}
        activePromptId="u1"
        onSelect={onSelect}
      />
    );

    const rail = screen.getByRole('navigation', { name: 'Prompt history' });
    expect(rail).toHaveTextContent('Prompts2');
    expect(screen.getAllByRole('listitem')).toHaveLength(2);
    expect(screen.getByRole('listitem', { name: /Inspect the shell/ })).toHaveAttribute('aria-current', 'true');

    await user.click(screen.getByRole('listitem', { name: /Run the visual smoke test/ }));
    expect(onSelect).toHaveBeenCalledWith('u2');
  });

  it('shows a truthful empty state for a new session', () => {
    render(<PromptHistoryRail messages={[]} activePromptId={null} onSelect={vi.fn()} />);
    expect(screen.getByText('Your prompts appear here.')).toBeInTheDocument();
  });
});
