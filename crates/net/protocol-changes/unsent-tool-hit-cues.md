`CueKind::HammerHit` and `CueKind::WrenchHit` are removed: nothing sent them
(tool hits play their sound through `CueKind::WeaponSound`), so later `CueKind`
variants move up by two.
