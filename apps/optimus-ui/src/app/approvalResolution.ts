import type { ToolApprovalBinding } from '../ipc/contracts';
import type { ComposerSettings } from '../state/composerStore';

export function approvalResolutionParams(
  sessionId: string,
  binding: ToolApprovalBinding,
  decision: 'approve' | 'deny',
  composer: ComposerSettings,
  projectId?: string
): Record<string, unknown> {
  return {
    session_id: sessionId,
    run_id: binding.run_id,
    call_id: binding.call_id,
    job_id: binding.job_id,
    node_id: binding.node_id,
    node_index: binding.node_index,
    effect_sha256: binding.effect_sha256,
    decision,
    provider: composer.provider,
    model: composer.model,
    access: composer.access,
    ...(projectId ? { project_id: projectId } : {}),
  };
}
