# Phase 15 — multi-agent durable campaigns

Date: 2026-07-19  
Priority: function > Hermes; UI polish last.  
Testing: deterministic isolation (unique homes, no wall sleeps, explicit seeds).

## Delivered

Ordered multi-agent **campaigns** on the Work Graph:

- Campaign tables in `optimus.db`; phase 15 introduced schema v3 and ADR-0020
  advances current persistence to schema v4 with fenced execution leases
  `campaigns.db` import  
- Each step → Work Graph job (`WriteFile` or SmartDeny `RunCommand`)  
- Deterministic step/job identity, atomic job creation, and job-derived status  
- Sequential run/resume with targeted crashed-node recovery  
- Blocks on approvals; continues after grant  
- Non-executing `campaign diagnose` and deterministic projection repair  

### CLI

```bash
optimus campaign create multi \
  --write agents/a.txt=alpha \
  --write agents/b.txt=beta
optimus campaign run <id>
optimus campaign status <id>
optimus campaign list
optimus campaign diagnose --json
optimus campaign repair --json
```

### Eval suite expanded

`optimus eval run` now 4 deterministic cases (+ write_file job).

## Evidence

```text
campaign::sequential_write_campaign_succeeds ... ok
campaign::run_command_blocks_then_grant_resumes_campaign ... ok
campaign::crash_after_job_creation_resumes_the_same_deterministic_job ... ok
campaign::crash_with_running_node_is_recovered_before_campaign_resume ... ok
campaign::campaign_status_is_derived_from_the_work_graph_authority ... ok
CLI create+run => Succeeded, workspace files alpha/beta
eval run => passed=4 failed=0
playwright 10/10
install relaunch pid on Programs\OptimusAgent
```

## Why this beats Hermes process-local multi-agent

Hermes subagents die with the process. Optimus campaign steps are Work Graph jobs with
approval ledger + resume — operator can grant and continue after crash.
