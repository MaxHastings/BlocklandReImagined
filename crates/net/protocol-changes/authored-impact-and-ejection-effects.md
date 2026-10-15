Image definitions may provide a sprite/composite `shell_effect` instead of a
debris model. The host emits the existing WeaponShell cue for those images;
clients resolve it at the same image socket and play authored effects.
Projectile effects may select authored map material kinds. The cue shape is
unchanged, but clients need the new content semantics. Alpha compatibility
and migration remain out of scope.
