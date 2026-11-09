# Independent action look on movement input

Movement inputs can carry an optional bounded world yaw/pitch for gameplay
actions, using the same typed direction as reliable ActionAim. Mounted mouse
steering retains its own raw turn; initial and continuous personal-weapon fire
use action look instead of treating steering accumulators as weapon aim.
The server retains position, origin, permission and input-lease authority.
Controllers without an explicit view keep authoritative body-facing behavior.
This changes the movement wire shape; alpha peers require matching versions.
