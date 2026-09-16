# Agent Execution Policy

- The primary agent MUST complete every task on its own.
- The agent MUST NOT deploy, spawn, invoke, or delegate work to subagents in any form.
- The agent MUST NOT use delegation mechanisms such as `task`, `agent`, `workpool`, or anything similar that runs another agent.
- All research, planning, implementation, review, debugging, and verification MUST be done directly by the primary agent.
- Task size or complexity is not a reason to delegate. The primary agent keeps working on it alone until it is finished.
