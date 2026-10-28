# Destruction performance check — preview.8

This copy uses two constraint-solver iterations for client-only rubble instead
of four. Motion still updates at120Hz, with the same collisions, visible detail
and Physics Quality body limit. The goal is lower cost during dense destruction;
visual piles may settle a little differently. Logs retain the detailed timings.

1. Launch this copy and load the same large CTF/TDM build and match settings.
2. Keep the same Physics Quality and bots/weapons. Play normally until the
   slowdown returns, then continue for about30 seconds so several slow frames
   are logged. A video is optional; the log is the useful CPU evidence.
3. Quit normally and tell this chat the test is finished. Logs are in the
   preview folder's `logs/`; existing preferences and match recordings use the
   normal application data folder. No profiling toggle or manual log upload is
   needed on this PC.

Also watch the mast/large landing assemblies: they should stay recognizable,
collide with the deck/floor and respond when shot. Report visible sinking,
violent jitter or lost material alongside any remaining performance dip.

Slow-frame lines now separate incoming brick changes, hit queries, model
readiness, surroundings, assembly work, collision preparation and simulation.
Nested simulation entries include their fixed-step count. Slow advances note
loose/chunk body counts and visible section membership. This distinguishes a
costly solver step from several catch-up steps, delivery work or hit queries.

Compare the same scene/settings first. Lowering quality or changing the map
would make the next capture less useful for identifying this particular stall.
