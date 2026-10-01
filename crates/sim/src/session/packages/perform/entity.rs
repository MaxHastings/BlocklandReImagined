//! What the engine does for the `entity` operations
//! (`bri_package_runtime::ops::entity`).
use super::*;

impl Perform for ops::SpawnEntity {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::SpawnEntity {
            kind,
            position,
            vars,
        } = self;
        let id = session.spawn_package_entity(&kind, Vec3::from(position))?;
        if let Some(e) = session
            .packages
            .as_mut()
            .and_then(|h| h.entities.get_mut(&id))
        {
            e.vars = vars;
        }
        Ok(())
    }
}
impl Perform for ops::RemoveEntity {
    fn entity(&self) -> Option<u64> {
        Some(self.entity)
    }
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::RemoveEntity { entity } = self;
        session.remove_package_entity(entity);
        Ok(())
    }
}
impl Perform for ops::Steer {
    fn entity(&self) -> Option<u64> {
        Some(self.entity)
    }
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::Steer {
            entity,
            direction,
            jump,
        } = self;
        if let Some(e) = session
            .packages
            .as_mut()
            .and_then(|h| h.entities.get_mut(&entity))
        {
            e.steer = (Vec3::new(direction[0], 0.0, direction[1]), jump);
        }
        Ok(())
    }
}
impl Perform for ops::Label {
    fn entity(&self) -> Option<u64> {
        Some(self.entity)
    }
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::Label { entity, label } = self;
        if let Some(e) = session
            .packages
            .as_mut()
            .and_then(|h| h.entities.get_mut(&entity))
        {
            e.label = label;
        }
        Ok(())
    }
}
