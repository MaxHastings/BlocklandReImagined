Admin server settings and their UI accept Max bots up to 64 (default 16).
The authoritative admission and targeted sight budgets share that capacity.

The combined active/loading participant budget is 128, leaving the existing
64 human connection limit independent from the bot budget. Admission, public
package state, minigame membership, weapons, vitals, appearance and inventory
validation use the same combined budget. Replicas reject overlapping loading
and active identities and combined rosters over that budget.
