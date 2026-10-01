//! What the engine does for the `brick_events` operations
//! (`bri_package_runtime::ops::brick_events`).
use super::*;

impl Perform for ops::SetBrickField {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::SetBrickField { brick, key, value } = self;
        session.package_set_brick_field(package, brick, &key, value)
    }
}
impl Perform for ops::FireBrickInput {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::FireBrickInput {
            brick,
            input,
            player,
        } = self;
        session.package_fire_brick_input(package, brick, &input, player)
    }
}
impl Perform for ops::FireGameInput {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::FireGameInput {
            game,
            input,
            player,
            killer,
        } = self;
        session.package_fire_game_input(package, game, &input, player, killer)
    }
}
