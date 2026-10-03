`ToolAction::SetEvents` and replicated `Brick::events` rows now execute delayed
Projectile Delete/Bounce/Redirect/Explode against the original live projectile,
checking IF when due and safely skipping missing targets. Immediate outputs
retain their existing behavior.
