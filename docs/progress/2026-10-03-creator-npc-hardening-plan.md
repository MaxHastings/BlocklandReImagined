# 2026-10-03 Creator usability, NPC consolidation and Shark planning

Maxwell requested a broad next-pass plan after the v0.2.1 handoff: reduce
Wrench Events choice overload, make MiniGame teams quick to author, consolidate
NPC behavior/performance, and include incomplete Shark behavior explicitly.

Root coordinated three existing GPT-6.1 Sol High agents for read-only audits.
They verified flattened event choices, 18 Slayer team-specific variants,
whole-editor expansion from one IF row, and MiniGame Add Team after all settings
and full team details. Shark remains a partial policy/model port, not completed
behavior parity. Sustained current-policy Windows performance and original crash
attribution remain open.

The concrete objectives, lane ownership, definitions of done, sequence and
stop rules are in [the hardening plan](../audits/creator-and-npc-hardening-plan.md).
Legacy team inputs must not be silently transformed into delayed IF guards;
grouped presentation preserves semantics. Shared UI controls and actor/runtime
integration stay root-owned. Small supported multi-step objectives, including
the measured eight-switch ceiling, are a bounded reliability target; broader
tool inference, cooperative stacking and a new UI framework are excluded.

Evidence commands: `git status --short`, targeted `rg` over event models,
Wrench layout, MiniGame layout, Slayer registration and Shark port; reads of
STATUS/product principles/current publication and GUI/bot/performance audits.
An initial unquoted shell glob did not match event source files; corrected to
explicit paths. No code builds, source patches or interactive tests ran.
Only this progress entry and the plan were added; existing local setup note
remains untouched. This records a proposed next pass, not implemented fixes or
publication of another release. Next work is implementation against the task
matrix, independent review, required gate/CI and a Maxwell playtest candidate.
