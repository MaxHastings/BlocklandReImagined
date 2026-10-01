//! Add-Ons' wrench event inputs (`behaviour.json` `brick_inputs`, v20's
//! `registerInputEvent`): Slayer_CTF's `onFlagPickedUp` and the like.
//! Builders wire them to outputs in the wrench like the engine's own
//! inputs; the Add-On's rules fire them with `fire_brick_input`, and the
//! rows run as the brick owner's, under the same budgets and trust.
use super::*;
use bri_events as ev;
use bri_package_runtime::content::BRICK_INPUT_TARGETS;

impl Session {
    /// The running Add-Ons' inputs, as the event catalog lists them. Sent
    /// to players so their wrench offers them too.
    pub fn package_brick_inputs(&self) -> Vec<ev::InputDef> {
        let Some(host) = self.packages.as_ref() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (package, behaviour) in host.catalog.behaviours() {
            for input in &behaviour.brick_inputs {
                let mut targets = vec![("Self".to_owned(), "fxDTSBrick".to_owned())];
                targets.extend(
                    BRICK_INPUT_TARGETS
                        .iter()
                        .filter(|(slot, _)| input.targets.iter().any(|t| t == slot))
                        .map(|(slot, class)| ((*slot).to_owned(), (*class).to_owned())),
                );
                out.push(ev::InputDef {
                    id: format!("{package}:{}", input.name),
                    class_name: "fxDTSBrick".into(),
                    name: input.name.clone(),
                    targets,
                    source: package.clone(),
                    source_line: 0,
                });
            }
        }
        out
    }

    /// `fire_brick_input`: run the rows on `brick` wired to `input`, one of
    /// `package`'s own inputs. Without a wrench event catalog (a host that
    /// runs no events) nothing happens.
    pub(in crate::session) fn package_fire_brick_input(
        &mut self,
        package: &str,
        brick: BrickId,
        input: &str,
        player: Option<OwnerId>,
    ) -> Result<()> {
        let host = self.packages.as_ref().context("No packages are enabled")?;
        let declared = host
            .catalog
            .behaviours()
            .find(|(id, _)| *id == package)
            .and_then(|(_, b)| {
                b.brick_inputs
                    .iter()
                    .find(|i| i.name.eq_ignore_ascii_case(input))
            })
            .map(|i| i.name.clone())
            .with_context(|| format!("`{input}` is not one of `{package}`'s brick_inputs"))?;
        ensure!(
            self.simulation.state().bricks.contains_key(&brick),
            "No such brick"
        );
        if let Some(p) = player {
            ensure!(self.peers.contains_key(&p), "No such player");
        }
        self.fire_input(brick, &declared, player);
        Ok(())
    }
}
