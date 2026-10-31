# Movement delay: local reset trace

The long-session local-feel report remains unresolved. The earlier guest
input-admission reproduction and its fix do not establish this symptom's cause.
The installed v0.4.2 logs do not contain the later preview's `Movement health`
records; the two preserved preview-assembly folders also yielded no such text
records in their log/text files. This is a search of those locations, not proof
that no capture exists elsewhere.

Source trace: ordinary `Motion::advance` clamps elapsed time, advances a bounded
fixed-step accumulator and exponentially decays visual correction. A changed
`spawn_tick` clears presentation correction and resets presentation state, but
for an unmounted predictor it subsequently calls `Predictor::reconcile`, which
discards acknowledged inputs and replays remaining motor inputs. It does not
generally clear pending inputs the way `Predictor::teleport` does. Mounted poses
do take the teleport path. A session reset replaces all motion state. Thus the
observation that respawn helps is not evidence that all input history was reset.
Replaying unacknowledged controls can be correct after respawn; changing that
behavior without correlating server queue/ack and input timing risks dropping
valid post-spawn controls. The local correction and accumulator are bounded,
but that alone does not rule out a persistent offset under repeated corrections.

The existing ten-second health record includes the predictor's pending count,
sequence/ack trail, correction peak and host/local pose, but does not capture
all host queue/admission state or time of the subjective transition. Next use
available recordings and automated local/host transition scenarios to correlate
the actual sequence and body lifecycle; preserve the distinction between a
plausible mechanism and a demonstrated cause. No further interactive
playtest is requested from Maxwell.
