import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

describe('motion contract', () => {
  it('contains none of the forbidden expensive animation declarations', () => {
    const css = readFileSync(resolve(process.cwd(), 'src/styles.css'), 'utf8');
    expect(css).not.toMatch(/transition\s*:\s*all/i);
    expect(css).not.toMatch(/backdrop-filter/i);
    expect(css).not.toMatch(/will-change/i);
    expect(css).not.toMatch(/filter\s*:\s*blur/i);
  });
});
