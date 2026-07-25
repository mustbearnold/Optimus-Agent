import { describe, expect, it } from 'vitest';
import { composeSendMessage } from './composeSendMessage';

describe('composeSendMessage', () => {
  it('sends annotation alone when input is empty', () => {
    expect(composeSendMessage('', 'Preview context (untrusted): x')).toBe(
      'Preview context (untrusted): x'
    );
  });

  it('joins input and annotation for send', () => {
    expect(composeSendMessage('please review', 'Preview context: button')).toBe(
      'please review\n\nPreview context: button'
    );
  });

  it('returns empty when both blank', () => {
    expect(composeSendMessage('  ', '')).toBe('');
  });
});
