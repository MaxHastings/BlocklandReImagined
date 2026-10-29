# 2026-10-29 Tutorial open-air ambience correction

Maxwell supplied an overhead gameplay capture of Tutorial. It shows open
roof framing, exposed instructional routes and grassy enclosing slopes. This
corrects the initial close-up thumbnail-based room-tone suggestion: Tutorial's
baseline should be very quiet outdoor air, with only local wall/roof shelter.
Open rafters must not classify the whole structure as a sealed room.

Inspected the supplied full-resolution PNG and retained an unchanged copy only
in ignored `artifacts/map-ambience/inputs/tutorial-user-overview.png`, SHA256
`3b19c358430e9e861eca065aa004bdae2047cd4f7e5c254e756086e80cd578b6`.
`tools/map_ambience/maps.json` pins it and records the correction. The offline
audition report now shows this overview for Tutorial and offers outdoor air.
The original thumbnail remains pinned; an absent supplemental capture is
explicitly labelled when reproducing the report elsewhere.

Rebuilt the offline audition successfully using its documented command.
No game window, interactive input or new offscreen render was needed: Maxwell's
capture supplies the relevant evidence. This is a visual correction, not
in-game audio or subjective listening acceptance.
