---
name: troubleshooting
description: Troubleshoot reproducible bugs, failures, and regressions.
---

# Definition of Ready
Bug report quality check:
- Identify proper branch / tag / version of the product
- Actual behaviour description, logs, outputs etc.

# Procedure
- Start with input — a symptom report, not a diagnosis, collect and index references
- Code exploration — locate the involved paths, build a working model
- Check it is reproducible — deterministic repro before any change
- Propose hypotheses — rank candidates, then falsify them
- Run the fix in a separate worktree — isolated branch, gates, clean diff
- Pick the winning fix — apply a clean implementation on a fresh fix branch in the main worktree
