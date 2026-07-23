import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { OptimusApp } from './OptimusApp';

describe('OptimusApp fixture contract', () => {
  it('renders the dense workbench and honest capability boundaries', async () => {
    render(<OptimusApp />);
    expect(await screen.findByRole('complementary', { name: 'Projects and sessions' })).toBeInTheDocument();
    expect(screen.getByRole('log', { name: 'Conversation' })).toBeInTheDocument();
    expect(screen.getByRole('complementary', { name: 'Evidence workspace' })).toBeInTheDocument();
    expect(screen.getByLabelText('Message Optimus')).toBeInTheDocument();
  });
});
