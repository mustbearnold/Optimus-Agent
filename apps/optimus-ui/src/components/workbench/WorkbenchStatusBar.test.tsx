import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { autoComposer } from '../../state/composerStore';
import { WorkbenchStatusBar } from './WorkbenchStatusBar';

describe('WorkbenchStatusBar', () => {
  it('surfaces live run state and durable session settings without inventing usage data', () => {
    render(
      <WorkbenchStatusBar
        status="awaiting_approval"
        statusText="Run the focused verification command"
        settings={autoComposer}
        project={null}
      />
    );

    expect(screen.getByRole('contentinfo', { name: 'Session status' })).toHaveTextContent(
      'Approval needed'
    );
    expect(screen.getByText('Run the focused verification command')).toBeInTheDocument();
    expect(screen.getByText('Auto')).toBeInTheDocument();
    expect(screen.getByText('High')).toBeInTheDocument();
    expect(screen.getByText('Standard')).toBeInTheDocument();
    expect(screen.queryByText(/tokens|cache|cost/i)).not.toBeInTheDocument();
  });
});
