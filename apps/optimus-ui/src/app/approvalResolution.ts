import type { ApprovalResolveRequest, ToolApprovalBinding } from '../ipc/contracts';
export function approvalResolutionParams(
  sessionId: string,
  binding: ToolApprovalBinding,
  decision: 'approve' | 'deny',
  projectId?: string
): ApprovalResolveRequest {
  return {
    session_id: sessionId,
    run_id: binding.run_id,
    call_id: binding.call_id,
    job_id: binding.job_id,
    node_id: binding.node_id,
    node_index: binding.node_index,
    effect_sha256: binding.effect_sha256,
    decision,
    ...(projectId ? { project_id: projectId } : {}),
  };
}
