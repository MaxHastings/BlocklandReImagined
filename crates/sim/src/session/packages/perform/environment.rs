//! What the engine does for the `environment` operations
//! (`bri_package_runtime::ops::environment`).
use super::*;

impl Perform for ops::SetEnvironment {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::SetEnvironment { changes, unset } = self;
        let tick = session.simulation.state().tick;
        let host = session
            .packages
            .as_mut()
            .context("No packages are enabled")?;
        let origin = package.to_string();
        ensure!(
            host.shares.environment.available(&origin, tick) >= 1,
            "Dropped: more than {PACKAGE_ENVIRONMENT_CHANGES} environment changes a second"
        );
        host.shares.environment.spend(&origin, tick, 1);
        let mut settings = session.environment();
        settings.merge(&changes);
        for key in &unset {
            ensure!(settings.unset(key), "No environment setting {key}");
        }
        session.set_environment(settings)
    }
}
