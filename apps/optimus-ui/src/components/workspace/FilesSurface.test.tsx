import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { FilesSurface } from './FilesSurface';

describe('FilesSurface bootstrap', () => {
  it('renders with a null transport without crashing (bootstrap window)', () => {
    // Regression: the packaged renderer mounts with transport=null while the
    // spec-015 A3 broker ticket is awaited; the mount-time fs_list load must
    // not throw.
    render(<FilesSurface transport={null} active />);

    expect(screen.getByLabelText('Files')).toBeInTheDocument();
  });
});
