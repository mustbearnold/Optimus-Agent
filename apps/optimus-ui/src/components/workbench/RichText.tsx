import { createElement, type ReactNode } from 'react';

type RichTextProps = {
  content: string;
};

type Block =
  | { kind: 'paragraph'; lines: string[] }
  | { kind: 'heading'; level: number; text: string }
  | { kind: 'unordered-list'; items: string[] }
  | { kind: 'ordered-list'; start: number; items: string[] }
  | { kind: 'blockquote'; lines: string[] }
  | { kind: 'code'; language: string; text: string }
  | { kind: 'rule' };

const inlineToken = /(`[^`\n]+`|\[([^\]\n]+)\]\((https?:\/\/[^\s)]+)\)|\*\*([^*\n]+)\*\*|__([^_\n]+)__|\*([^*\n]+)\*|_([^_\n]+)_)/g;

export function RichText({ content }: RichTextProps) {
  const blocks = parseBlocks(content);
  return (
    <div className="rich-text">
      {blocks.map((block, index) => renderBlock(block, index))}
    </div>
  );
}

function parseBlocks(content: string): Block[] {
  const lines = content.replace(/\r\n?/g, '\n').split('\n');
  const blocks: Block[] = [];
  let index = 0;

  while (index < lines.length) {
    const line = lines[index] || '';
    if (!line.trim()) {
      index += 1;
      continue;
    }

    const fence = line.match(/^\s*```\s*([\w-]*)\s*$/);
    if (fence) {
      const codeLines: string[] = [];
      index += 1;
      while (index < lines.length && !/^\s*```\s*$/.test(lines[index] || '')) {
        codeLines.push(lines[index] || '');
        index += 1;
      }
      if (index < lines.length) index += 1;
      blocks.push({ kind: 'code', language: fence[1] || '', text: codeLines.join('\n') });
      continue;
    }

    const heading = line.match(/^\s{0,3}(#{1,6})\s+(.+?)\s*#*\s*$/);
    if (heading) {
      blocks.push({ kind: 'heading', level: heading[1].length, text: heading[2] });
      index += 1;
      continue;
    }

    if (/^\s{0,3}(?:\*\s*){3,}$/.test(line) || /^\s{0,3}(?:-\s*){3,}$/.test(line) || /^\s{0,3}(?:_\s*){3,}$/.test(line)) {
      blocks.push({ kind: 'rule' });
      index += 1;
      continue;
    }

    if (/^\s*[-+*]\s+/.test(line)) {
      const items: string[] = [];
      while (index < lines.length) {
        const match = (lines[index] || '').match(/^\s*[-+*]\s+(.+)$/);
        if (!match) break;
        items.push(match[1]);
        index += 1;
      }
      blocks.push({ kind: 'unordered-list', items });
      continue;
    }

    const ordered = line.match(/^\s*(\d+)[.)]\s+(.+)$/);
    if (ordered) {
      const items: string[] = [ordered[2]];
      const start = Number(ordered[1]);
      index += 1;
      while (index < lines.length) {
        const match = (lines[index] || '').match(/^\s*\d+[.)]\s+(.+)$/);
        if (!match) break;
        items.push(match[1]);
        index += 1;
      }
      blocks.push({ kind: 'ordered-list', start, items });
      continue;
    }

    if (/^\s*>/.test(line)) {
      const quote: string[] = [];
      while (index < lines.length && /^\s*>/.test(lines[index] || '')) {
        quote.push((lines[index] || '').replace(/^\s*>\s?/, ''));
        index += 1;
      }
      blocks.push({ kind: 'blockquote', lines: quote });
      continue;
    }

    const paragraph: string[] = [line];
    index += 1;
    while (index < lines.length) {
      const next = lines[index] || '';
      if (!next.trim() || isBlockStart(next)) break;
      paragraph.push(next);
      index += 1;
    }
    blocks.push({ kind: 'paragraph', lines: paragraph });
  }

  return blocks;
}

function isBlockStart(line: string) {
  return /^\s*```\s*/.test(line)
    || /^\s{0,3}#{1,6}\s+/.test(line)
    || /^\s*[-+*]\s+/.test(line)
    || /^\s*\d+[.)]\s+/.test(line)
    || /^\s*>/.test(line)
    || /^\s{0,3}(?:\*\s*){3,}$/.test(line)
    || /^\s{0,3}(?:-\s*){3,}$/.test(line)
    || /^\s{0,3}(?:_\s*){3,}$/.test(line);
}

function renderBlock(block: Block, key: number): ReactNode {
  switch (block.kind) {
    case 'heading': {
      return createElement(`h${block.level}`, { key }, renderInline(block.text, `${key}-heading`));
    }
    case 'unordered-list':
      return (
        <ul key={key}>
          {block.items.map((item, index) => <li key={index}>{renderInline(item, `${key}-${index}`)}</li>)}
        </ul>
      );
    case 'ordered-list':
      return (
        <ol key={key} start={block.start}>
          {block.items.map((item, index) => <li key={index}>{renderInline(item, `${key}-${index}`)}</li>)}
        </ol>
      );
    case 'blockquote':
      return <blockquote key={key}>{renderInline(block.lines.join('\n'), `${key}-quote`)}</blockquote>;
    case 'code':
      return (
        <pre key={key} className={block.language ? `rich-code language-${block.language}` : 'rich-code'}>
          <code>{block.text}</code>
        </pre>
      );
    case 'rule':
      return <hr key={key} />;
    case 'paragraph':
      return <p key={key}>{renderInline(block.lines.join('\n'), `${key}-paragraph`)}</p>;
  }
}

function renderInline(value: string, keyPrefix: string): ReactNode[] {
  const nodes: ReactNode[] = [];
  let cursor = 0;
  let tokenIndex = 0;
  // Nested strong/emphasis/link content must not reset the parent scanner.
  const tokenPattern = new RegExp(inlineToken.source, 'g');

  for (let match = tokenPattern.exec(value); match; match = tokenPattern.exec(value)) {
    if (match.index > cursor) {
      nodes.push(...renderPlainText(value.slice(cursor, match.index), `${keyPrefix}-text-${tokenIndex}`));
    }
    const token = match[0];
    if (token.startsWith('`')) {
      nodes.push(<code key={`${keyPrefix}-code-${tokenIndex}`}>{token.slice(1, -1)}</code>);
    } else if (token.startsWith('[')) {
      nodes.push(
        <a
          key={`${keyPrefix}-link-${tokenIndex}`}
          href={match[3]}
          target="_blank"
          rel="noreferrer"
        >
          {renderInline(match[2] || '', `${keyPrefix}-link-${tokenIndex}`)}
        </a>
      );
    } else if (token.startsWith('**') || token.startsWith('__')) {
      nodes.push(<strong key={`${keyPrefix}-strong-${tokenIndex}`}>{renderInline(token.slice(2, -2), `${keyPrefix}-strong-${tokenIndex}`)}</strong>);
    } else {
      nodes.push(<em key={`${keyPrefix}-em-${tokenIndex}`}>{renderInline(token.slice(1, -1), `${keyPrefix}-em-${tokenIndex}`)}</em>);
    }
    cursor = match.index + token.length;
    tokenIndex += 1;
  }
  if (cursor < value.length) {
    nodes.push(...renderPlainText(value.slice(cursor), `${keyPrefix}-text-${tokenIndex}`));
  }
  return nodes;
}

function renderPlainText(value: string, keyPrefix: string): ReactNode[] {
  return value.split('\n').flatMap((line, index) =>
    index ? [<br key={`${keyPrefix}-br-${index}`} />, line] : [line]
  );
}
