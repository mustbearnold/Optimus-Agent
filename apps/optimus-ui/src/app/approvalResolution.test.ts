import { describe, expect, it } from 'vitest';
import { codexComposer } from '../state/composerStore';
import { approvalResolutionParams } from './approvalResolution';

const binding = {
  run_id: '11111111-1111-4111-8111-111111111111',
  call_id: 'call-1',
  tool_id: 'terminal',
  job_id: '22222222-2222-4222-8222-222222222222',
  node_id: '33333333-3333-4333-8333-333333333333',
  node_index: 4,
  effect_sha256: 'a'.repeat(64),
  summary: 'Run the command',
};

describe('approval continuation params', () => {
  it('keeps the paused turn under the selected provider and access profile', () => {
    expect(
      approvalResolutionParams(
        '44444444-4444-4444-8444-444444444444',
        binding,
        'approve',
        { ...codexComposer, access: 'full_project' },
        'project-a'
      )
    ).toEqual({
      session_id: '44444444-4444-4444-8444-444444444444',
      run_id: binding.run_id,
      call_id: binding.call_id,
      job_id: binding.job_id,
      node_id: binding.node_id,
      node_index: binding.node_index,
      effect_sha256: binding.effect_sha256,
      decision: 'approve',
      provider: 'codex',
      model: 'gpt-5.6-terra',
      access: 'full_project',
      project_id: 'project-a',
    });
  });

  it('falls closed when the paused turn predates this app lifecycle', () => {
    const params = approvalResolutionParams('session-a', binding, 'deny', undefined);
    expect(params).not.toHaveProperty('provider');
    expect(params).not.toHaveProperty('model');
    expect(params).not.toHaveProperty('access');
    expect(params).not.toHaveProperty('project_id');
  });
});
