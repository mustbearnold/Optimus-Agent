/** Merge composer input with gallery-promoted annotation for chat send. */
export function composeSendMessage(input: string, annotation: string): string {
  return [input.trim(), annotation.trim()].filter(Boolean).join('\n\n');
}
