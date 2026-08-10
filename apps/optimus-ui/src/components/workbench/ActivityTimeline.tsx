import { useEffect, useId, useState } from 'react';
import type { ToolActivity, ToolApprovalBinding } from '../../ipc/contracts';
import { Icon, type IconName } from '../chrome/Icon';

type ToolCategory = 'read' | 'command' | 'search' | 'edit' | 'browser' | 'other';
export type ApprovalDecision = 'approve' | 'deny';
export type ToolApprovalRequest = ToolApprovalBinding;
export type ApprovalDecisionHandler = (
  approval: ToolApprovalRequest,
  decision: ApprovalDecision,
  /** When set, grant session consent for this CommandClass before resolving. */
  grantClass?: string
) => void | Promise<void>;

export function ActivityTimeline({
  tools,
  onApprovalDecision,
}: {
  tools: ToolActivity[];
  onApprovalDecision?: ApprovalDecisionHandler;
}) {
  const summary = summarizeTools(tools);
  // R11: tool-to-tool gap breakdown — idle waits between consecutive tools,
  // computed from the kernel's timing events (start/finish run offsets).
  const gaps = tools.map((tool, index) => {
    const next = tools[index + 1];
    if (!next) return undefined;
    if (typeof tool.finishedAtMs !== 'number' || typeof next.startedAtMs !== 'number') {
      return undefined;
    }
    const gap = next.startedAtMs - tool.finishedAtMs;
    return gap >= 0 ? gap : undefined;
  });
  const gapTotal = gaps
    .filter((gap): gap is number => typeof gap === 'number')
    .reduce((sum, gap) => sum + gap, 0);
  const failedCount = tools.filter((tool) => isFailed(tool.status)).length;
  // A group is failed when the group failed. One denied call among twenty that
  // succeeded is a partial outcome, and calling it `is-failed` is what made the
  // collapsed header report a whole step as failed while every call after it
  // worked.
  const failed = failedCount > 0 && failedCount === tools.length;
  const partiallyFailed = failedCount > 0 && !failed;
  const attention = tools.some((tool) => isAttention(tool.status));
  const running = tools.some((tool) => isActive(tool.status));
  const [approvalStates, setApprovalStates] = useState<Record<string, ApprovalState>>({});
  const [openDetails, setOpenDetails] = useState<Record<string, boolean>>({});
  const [open, setOpen] = useState(attention);
  const rowsId = `activity-rows-${useId().replaceAll(':', '')}`;

  useEffect(() => {
    if (attention) setOpen(true);
  }, [attention]);

  if (!tools.length) return null;

  return (
    <div
      className={`activity-timeline${running ? ' is-running' : ''}${attention ? ' is-attention' : ''}${failed ? ' is-failed' : ''}${partiallyFailed ? ' has-failure' : ''}`}
      data-open={open ? 'true' : 'false'}
      data-tool-count={tools.length}
      aria-label="Tool activity"
    >
      <button
        type="button"
        className="activity-heading"
        aria-expanded={open}
        aria-controls={rowsId}
        onClick={() => setOpen((current) => !current)}
      >
        {/* A partial failure still earns the warning mark — it is the summary
            text, not the icon, that was claiming more than happened. */}
        <Icon name={failed || partiallyFailed || attention ? 'warning' : 'source'} />
        <span>
          {summary}
          {gapTotal > 0 ? ` · ${formatDuration(gapTotal)} idle between tools` : ''}
        </span>
        <Icon className="activity-chevron" name="chevron" />
      </button>
      <div className="activity-rows" id={rowsId} aria-hidden={!open}>
        <div className="activity-rows-inner">
          {tools.map((tool, index) => {
            const category = categorizeTool(tool.name);
            const approvalState = approvalStates[tool.id];
            const detailOpen = Boolean(openDetails[tool.id]);
            const detailId = `${rowsId}-detail-${tool.id.replace(/[^a-zA-Z0-9_-]/g, '')}`;
            return (
              <div className="activity-item" key={tool.id} data-tool-id={tool.id}>
                <button
                  type="button"
                  className={`activity-row is-${tool.status}`}
                  aria-expanded={detailOpen}
                  aria-controls={detailId}
                  aria-label={`Expand ${category} tool details`}
                  title={`${tool.name}: ${tool.detail}`}
                  onClick={() => setOpenDetails((current) => ({ ...current, [tool.id]: !detailOpen }))}
                >
                  <Icon name={toolIcon(category, tool.status)} />
                  <strong>{toolLabel(category, tool.name, tool.status)}</strong>
                  <span title={tool.detail || undefined}>
                    {tool.detail}
                    {typeof tool.durationMs === 'number' ? ` · ${formatDuration(tool.durationMs)}` : ''}
                  </span>
                </button>
                {detailOpen ? (
                  <div className="activity-detail" id={detailId} role="region" aria-label={`Technical details for ${tool.name}`}>
                    <dl>
                      <div><dt>Tool</dt><dd>{tool.name}</dd></div>
                      <div><dt>Status</dt><dd>{tool.status}</dd></div>
                      <div><dt>Call</dt><dd>{tool.callId}</dd></div>
                      {typeof tool.durationMs === 'number' ? <div><dt>Duration</dt><dd>{formatDuration(tool.durationMs)}</dd></div> : null}
                      {typeof gaps[index] === 'number' ? <div><dt>Idle after</dt><dd>{formatDuration(gaps[index]!)}</dd></div> : null}
                    </dl>
                    {tool.outcome ? <pre>{JSON.stringify(tool.outcome, null, 2)}</pre> : null}
                  </div>
                ) : null}
                {tool.status === 'awaiting_approval' && tool.approval ? (
                  <ToolApprovalCard
                    tool={tool}
                    state={approvalState}
                    onDecision={onApprovalDecision}
                    onStateChange={(next) =>
                      setApprovalStates((current) => ({ ...current, [tool.id]: next }))
                    }
                  />
                ) : null}
                {typeof gaps[index] === 'number' ? (
                  <div className="activity-gap" data-gap-ms={gaps[index]}>
                    idle {formatDuration(gaps[index]!)}
                  </div>
                ) : null}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}

type ApprovalState = {
  pending?: ApprovalDecision;
  submitted?: ApprovalDecision;
  error?: string;
};

function ToolApprovalCard({
  tool,
  state,
  onDecision,
  onStateChange,
}: {
  tool: ToolActivity;
  state?: ApprovalState;
  onDecision?: ApprovalDecisionHandler;
  onStateChange: (state: ApprovalState) => void;
}) {
  const approval = tool.approval;
  if (!approval) return null;
  const pending = state?.pending;
  const disabled = Boolean(pending || !onDecision);
  // Session consent (spec-014 R7) is offered for command effects only; the
  // class discriminator arrives from the kernel, derived from the exact
  // effect, never from free text.
  const grantClass = approval.command_class;
  const [alwaysAllow, setAlwaysAllow] = useState(false);

  const choose = async (decision: ApprovalDecision) => {
    if (!onDecision || disabled) return;
    const grant = decision === 'approve' && alwaysAllow ? grantClass : undefined;
    onStateChange({ pending: decision });
    try {
      // Only pass the consent class when it was actually chosen, so callers
      // that branch on arity keep working (the handler signature is optional).
      if (grant) {
        await onDecision(approval, decision, grant);
      } else {
        await onDecision(approval, decision);
      }
      onStateChange({ submitted: decision });
    } catch (error) {
      onStateChange({ error: errorMessage(error) });
    }
  };

  return (
    <section className="activity-approval" aria-label={`Approval required for ${tool.name}`}>
      <strong>Approval required</strong>
      <p>{approval.summary}</p>
      <small>Optimus will only continue with this exact action after your choice.</small>
      {grantClass ? (
        <label className="activity-approval-consent">
          <input
            type="checkbox"
            checked={alwaysAllow}
            onChange={(event) => setAlwaysAllow(event.target.checked)}
            disabled={disabled}
          />
          <span>
            Always allow <code>{grantClass}</code> in this project (this session)
          </span>
        </label>
      ) : null}
      <div className="activity-approval-actions" role="group" aria-label="Approval decision">
        <button
          type="button"
          onClick={() => void choose('approve')}
          disabled={disabled}
        >
          {pending === 'approve' ? 'Approving…' : 'Approve and continue'}
        </button>
        <button
          type="button"
          className="is-deny"
          onClick={() => void choose('deny')}
          disabled={disabled}
        >
          {pending === 'deny' ? 'Denying…' : 'Deny'}
        </button>
      </div>
      {state?.submitted ? (
        <p className="activity-approval-status" role="status">
          {state.submitted === 'approve' ? 'Approval submitted.' : 'Denial submitted.'}
        </p>
      ) : null}
      {state?.error ? (
        <p className="activity-approval-error" role="alert">
          {state.error}
        </p>
      ) : null}
    </section>
  );
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : 'Unable to submit this approval decision.';
}

// The collapsed header is the only thing most steps are ever read by, so it has
// to say what actually happened. "— failed" for a group where one call was
// denied and twenty succeeded is not a summary of that group, and the reader has
// no way to tell it from a step where nothing worked.
function summarizeTools(tools: ToolActivity[]) {
  const running = tools.some((tool) => isActive(tool.status));
  const failedCount = tools.filter((tool) => isFailed(tool.status)).length;
  const allFailed = failedCount > 0 && failedCount === tools.length;
  const categories = [...new Set(tools.map((tool) => categorizeTool(tool.name)))];
  if (tools.some((tool) => tool.status === 'awaiting_approval')) return 'Approval required';
  if (running) {
    const summary = formatList(categories.map((category) => summaryPhrase(category, true)));
    return failedCount ? `${summary} — ${failedCount} failed so far` : summary;
  }
  const summary = formatList(categories.map((category) => summaryPhrase(category, false)));
  if (allFailed) return `${summary} — failed`;
  if (failedCount) return `${summary} — ${failedCount} of ${tools.length} failed`;
  if (tools.some((tool) => tool.status === 'ambiguous')) return `${summary} — outcome unknown`;
  if (tools.every((tool) => tool.status === 'cancelled')) return `${summary} — cancelled`;
  if (tools.every((tool) => tool.status === 'suppressed')) {
    return tools.length === 1 ? 'Tool call skipped' : 'Tool calls skipped';
  }
  return summary;
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
  if (isFailed(status) || isAttention(status)) return 'warning';
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
  if (status === 'awaiting_approval') return `Approve ${name}`;
  if (status === 'cancelled') return `Cancelled ${name}`;
  if (status === 'suppressed') return `Skipped ${name}`;
  if (status === 'ambiguous') return `Check ${name}`;
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

function isActive(status: ToolActivity['status']) {
  return status === 'running';
}

function isFailed(status: ToolActivity['status']) {
  return status === 'failed';
}

function isAttention(status: ToolActivity['status']) {
  return status === 'awaiting_approval' || status === 'ambiguous';
}

function formatDuration(durationMs: number) {
  if (durationMs < 1_000) return `${durationMs}ms`;
  return `${(durationMs / 1_000).toFixed(durationMs < 10_000 ? 1 : 0)}s`;
}
