Section delivery has explicit BrickSectionReady (actual committed count) and
BrickSectionRetired barriers. Clients stage material until a terminal Ready
record; restoration/retirement supersedes late members. Brick.damage_section
carries transient [started, group] ownership on hidden section members, including
a packed side column. It permits bounded recovery of missing presentation cues
from authoritative state. Saved originals and restored bricks have no section tag.

Presentation backlog is local: protect controls and one section seed, drop
redundant member cues, reconcile tagged material. Do not disconnect valid world
replication or misreport local losses as server cue-sequence losses. No moving
poses, rigid bodies, rubble collision or damage are replicated. All peers require
the new protocol fingerprint; alpha has no backward migration.
