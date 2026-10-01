`ControlObject::Path` and `Vitals::camera_path`: a rule's `follow_path` flies a
player's camera along knots (`PathCamera`); the client samples the path at
the server tick.
Also `ControlObject::Observer` and `ControlObject::Point` with
`Vitals::camera_point` (a rule's free and point cameras), and
`Command::ObserverButton` (a spectator's keys for the rules).
