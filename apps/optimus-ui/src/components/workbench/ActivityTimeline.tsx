import type { ToolActivity } from '../../ipc/contracts';
import { Icon, type IconName } from '../chrome/Icon';

type ToolCategory = 'read' | 'command' | 'search' | 'edit' | 'browser' | 'other';

export function ActivityTimeline({ tools }: { tools: ToolActivity[] }) {
  if (!tools.length) return null;
  const summary = summarizeTools(tools);
  const failed = tools.some((tool) => tool.status === 'failed');
  const running = tools.some((tool) => tool.status === 'running');

  return (
    <details
      className={`activity-timeline${running ? ' is-running' : ''}${failed ? ' is-failed' : ''}`}
    >
      <summary className="activity-heading">
        <Icon name={failed ? 'warning' : 'source'} />
        <span>{summary}</span>
        <Icon className="activity-chevron" name="chevron" />
      </summary>
      <div className="activity-rows">
        {tools.map((tool) => {
          const category = categorizeTool(tool.name);
          return (
            <div className={`activity-row is-${tool.status}`} key={tool.id}>
              <Icon name={toolIcon(category, tool.status)} />
              <strong>{toolLabel(category, tool.name, tool.status)}</strong>
              <span>{tool.detail}</span>
            </div>
          );
        })}
      </div>
    </details>
  );
}

function summarizeTools(tools: ToolActivity[]) {
  const running = tools.some((tool) => tool.status === 'running');
  const failed = tools.some((tool) => tool.status === 'failed');
  const categories = [...new Set(tools.map((tool) => categorizeTool(tool.name)))];
  const phrases = categories.map((category) => summaryPhrase(category, running));
  const summary = formatList(phrases);
  return failed && !running ? `${summary} — failed` : summary;
}

function categorizeTool(name: string): ToolCategory {
  const normalized = name.toLowerCase();
  if (/(?:^|_)(?:read|open|list)(?:_|$)|file_read|readfile/.test(normalized)) return 'read';
  if (/(?:shell|command|terminal|exec|run_command)/.test(normalized)) return 'command';
  if (/(?:search|find|grep|query)/.test(normalized)) return 'search';
  if (/(?:write|edit|patch|replace|create_file)/.test(normalized)) return 'edit';
  if (/(?:browser|navigate|click|screenshot)/.test(normalized)) return 'browser';
  return 'other';
}

function summaryPhrase(category: ToolCategory, running: boolean) {
  const phrases: Record<ToolCategory, [string, string]> = {
    read: ['Read files', 'Reading files'],
    command: ['Ran a command', 'Running a command'],
    search: ['Searched', 'Searching'],
    edit: ['Edited files', 'Editing files'],
    browser: ['Used the browser', 'Using the browser'],
    other: ['Called tools', 'Calling tools'],
  };
  return phrases[category][running ? 1 : 0];
}

function formatList(items: string[]) {
  if (items.length <= 1) return items[0] || 'Tool activity';
  if (items.length === 2) return `${items[0]}, ${lowercaseFirst(items[1]!)}`;
  return `${items.slice(0, -1).join(', ')}, and ${lowercaseFirst(items.at(-1)!)}`;
}

function lowercaseFirst(value: string) {
  return value.charAt(0).toLowerCase() + value.slice(1);
}

function toolIcon(category: ToolCategory, status: ToolActivity['status']): IconName {
  if (status === 'failed') return 'warning';
  const icons: Record<ToolCategory, IconName> = {
    read: 'source',
    command: 'terminal',
    search: 'search',
    edit: 'artifact',
    browser: 'browser',
    other: 'tasks',
  };
  return icons[category];
}

function toolLabel(
  category: ToolCategory,
  name: string,
  status: ToolActivity['status']
) {
  const active = status === 'running';
  const labels: Record<ToolCategory, [string, string]> = {
    read: ['Read file', 'Reading file'],
    command: ['Ran command', 'Running command'],
    search: ['Searched', 'Searching'],
    edit: ['Edited file', 'Editing file'],
    browser: ['Used browser', 'Using browser'],
    other: [`Called ${name}`, `Calling ${name}`],
  };
  return labels[category][active ? 1 : 0];
}
