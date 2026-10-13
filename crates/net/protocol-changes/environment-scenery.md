`Checkpoint::environment`, `Delta::environment` and `Action::SetEnvironment`:
the environment settings gain `sky` (a sky box, or `plain`), `ground` and
`ground_color` (a flat map's floor), and `water`, `water_height`,
`water_color` and `underwater_color` (a sea over a flat map, or `none`).
Each client draws them from its own content; the host lays the water over
the plate for swimming and lava.
