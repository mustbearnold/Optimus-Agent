import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { createOptimusClient } from '../../ipc/client';
import { FilesSurface } from './FilesSurface';

describe('FilesSurface bootstrap', () => {
  it('renders with a null-transport client without crashing (bootstrap window)', () => {
    // Regression: the packaged renderer mounts with transport=null while the
    // spec-015 A3 broker ticket is awaited; the mount-time fs_list load must
    // surface NoTransportError into the error slot, not throw.
    render(<FilesSurface client={createOptimusClient(null)} active />);

    expect(screen.getByLabelText('Files')).toBeInTheDocument();
  });
});
