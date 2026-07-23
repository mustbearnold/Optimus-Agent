import type { ToolActivity } from '../../ipc/contracts';
import { Icon } from '../chrome/Icon';

export function ActivityTimeline({
  tools,
  statusText,
}: {
  tools: ToolActivity[];
  statusText: string;
}) {
  if (!tools.length && !statusText) return null;
  return (
    <details className="activity-timeline" open>
      <summary className="activity-heading">
        <span className="activity-pulse" aria-hidden="true" />
        <span>{statusText || 'Activity'}</span>
        {tools.length ? <span className="activity-count">{tools.length}</span> : null}
      </summary>
      {tools.length ? (
        <div className="activity-rows">
          {tools.map((tool) => (
            <div className="activity-row" key={tool.id}>
              <Icon name={tool.status === 'failed' ? 'warning' : 'check'} />
              <strong>{tool.name}</strong>
              <span>{tool.detail}</span>
            </div>
          ))}
        </div>
      ) : null}
    </details>
  );
}
