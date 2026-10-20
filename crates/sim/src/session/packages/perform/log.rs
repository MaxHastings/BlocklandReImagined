//! What the engine does for the `log` operations
//! (`bri_package_runtime::ops::log`).
use super::*;

impl Perform for ops::Log {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::Log { name, text } = self;
        session.take_log_line(package)?;
        bri_console::echo(format!("Add-On {package} [{name}]: {text}"));
        Ok(())
    }
}
impl Perform for ops::Warn {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::Warn { name, text } = self;
        session.take_log_line(package)?;
        // `note` lists it and tells the host's log and admins once a minute
        // per Add-On and code, so the code carries the channel.
        let host = session
            .packages
            .as_mut()
            .context("No packages are enabled")?;
        note(
            host,
            Diagnostic::warning(&format!("script.warn.{name}"), format!("[{name}] {text}"))
                .at(package),
        );
        Ok(())
    }
}
