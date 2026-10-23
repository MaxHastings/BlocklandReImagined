//! Map volumes (`Scene::volumes`): named boxes the map authors, which rules
//! look up by point. The engine gives them no meaning of its own.
use super::*;
use bri_content::scene::{MAX_VOLUMES, Volume};

impl Session {
    /// Replace the authored map partition used by package point queries.
    pub fn set_map_regions(
        &mut self,
        regions: Option<bri_content::regions::RegionMap>,
    ) -> Result<()> {
        self.simulation.set_map_regions(regions)
    }
    /// Install the map's volumes (from `NativeMap`), highest priority first.
    pub fn set_volumes(&mut self, volumes: Vec<Volume>) -> Result<()> {
        ensure!(volumes.len() <= MAX_VOLUMES, "Too many map volumes");
        ensure!(
            volumes.iter().all(|v| v
                .center
                .iter()
                .chain(&v.half)
                .chain(&v.to_local)
                .all(|n| n.is_finite())),
            "Invalid map volume"
        );
        self.map_volumes = volumes;
        Ok(())
    }
    /// The volumes holding the world point `p`, highest priority first and
    /// then in map order.
    pub fn volumes_at(&self, p: [f32; 3]) -> Vec<&Volume> {
        self.map_volumes.iter().filter(|v| v.contains(p)).collect()
    }
}
