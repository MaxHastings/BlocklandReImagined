//! Package policy is evaluated off the audio packet path, in a read-only sandbox.
use super::*;
use crate::session::{VoiceConversation, VoicePlayback, VoiceRoute, VoiceSelection};

impl Session {
    pub fn voice_routes(
        &mut self,
        owners: &[OwnerId],
    ) -> BTreeMap<OwnerId, BTreeMap<OwnerId, VoicePlayback>> {
        self.voice_selections(owners)
            .into_iter()
            .map(|(listener, routes)| {
                (
                    listener,
                    routes
                        .into_iter()
                        .map(|(speaker, selection)| (speaker, selection.playback))
                        .collect(),
                )
            })
            .collect()
    }
    /// Evaluate one authoritative route selection, retaining its host-only
    /// conversation identity for grant/decoder lifetime decisions.
    pub fn voice_selections(
        &mut self,
        owners: &[OwnerId],
    ) -> BTreeMap<OwnerId, BTreeMap<OwnerId, VoiceSelection>> {
        let mut result = BTreeMap::new();
        if owners.len() > bri_admin::MAX_CONNECTIONS {
            return result;
        }
        let providers: Vec<_> = self
            .packages
            .as_ref()
            .map(|h| {
                h.catalog
                    .behaviours()
                    .filter(|(id, b)| {
                        b.on_voice
                            && h.catalog.packages[*id]
                                .manifest
                                .capabilities
                                .iter()
                                .any(|c| c == "voice")
                    })
                    .map(|(id, _)| id.clone())
                    .collect()
            })
            .unwrap_or_default();
        if providers.len() > 4 {
            note(
                self.packages.as_mut().unwrap(),
                Diagnostic::error(
                    "voice.providers",
                    "at most four voice policy providers may run together",
                ),
            );
            return result;
        }
        if providers.is_empty() {
            return result;
        }
        // Reuse the normal script view once per refresh, not once per audio frame.
        let snapshot = Arc::new(self.package_snapshot());
        let vars: BTreeMap<_, _> = providers
            .iter()
            .map(|p| (p.clone(), Arc::new(self.package_vars(p))))
            .collect();
        let tick = self.simulation.state().tick;
        let listeners = Dynamic::from_array(
            owners
                .iter()
                .map(|id| Dynamic::from_int(*id as i64))
                .collect(),
        );
        // A valid [] abstains. An error/budget failure cannot safely fall
        // through to a more permissive provider, including public voice.
        'speaker: for &speaker in owners {
            let mut chosen: BTreeMap<u64, (u8, Option<VoiceSelection>)> = BTreeMap::new();
            for package in &providers {
                let host = self.packages.as_mut().unwrap();
                if host.shares.work.available(package, tick) <= 0 {
                    continue 'speaker;
                }
                let state = host.store.namespace(package).cloned().unwrap_or_default();
                let vars = vars[package].clone();
                let world = super::super::script_world::ScriptWorld::new(self, package);
                let call = Call {
                    function: "voice_routes",
                    args: vec![Dynamic::from_int(speaker as i64), listeners.clone()],
                    budget: Budget::Objective,
                    snapshot: snapshot.clone(),
                    caller: Some(speaker),
                    aim: None,
                    entity: None,
                    state,
                    entity_vars: vars,
                    world: Some(&world),
                };
                let start = std::time::Instant::now();
                let answer = self.packages.as_ref().unwrap().runtime.query(package, call);
                let elapsed = start.elapsed();
                drop(world);
                let host = self.packages.as_mut().unwrap();
                *host.script_time.entry(package.clone()).or_default() += elapsed;
                let routes = answer.and_then(|o| {
                    bri_package_runtime::rhai::serde::from_dynamic::<serde_json::Value>(&o.returned)
                        .map_err(|e| Diagnostic::error("voice.routes", e.to_string()).at(package))
                        .and_then(|v| {
                            serde_json::from_value::<Vec<VoiceRoute>>(v).map_err(|e| {
                                Diagnostic::error("voice.routes", e.to_string()).at(package)
                            })
                        })
                });
                self.charge_work(package);
                let routes = match routes {
                    Ok(r) if r.len() <= owners.len() => r,
                    Ok(_) => {
                        note(
                            self.packages.as_mut().unwrap(),
                            Diagnostic::error("voice.routes", "too many voice recipients")
                                .at(package),
                        );
                        continue 'speaker;
                    }
                    Err(d) => {
                        note(self.packages.as_mut().unwrap(), d);
                        continue 'speaker;
                    }
                };
                for route in routes {
                    if route.conversation.len() > 96
                        || route.conversation.chars().any(char::is_control)
                    {
                        note(
                            self.packages.as_mut().unwrap(),
                            Diagnostic::error(
                                "voice.conversation",
                                "conversation is at most 96 bytes without control characters",
                            )
                            .at(package),
                        );
                        continue 'speaker;
                    }
                    let playback = VoicePlayback {
                        spatial: route.spatial,
                        range: route.range,
                        label: route.label,
                        effects: route.effects,
                    };
                    if !playback.valid() {
                        note(
                            self.packages.as_mut().unwrap(),
                            Diagnostic::error(
                                "voice.playback",
                                "invalid voice playback parameters",
                            )
                            .at(package),
                        );
                        continue 'speaker;
                    }
                    if route.listener == speaker || !owners.contains(&route.listener) {
                        continue;
                    }
                    let playback = route.hear.then_some(VoiceSelection {
                        playback,
                        conversation: (!route.conversation.is_empty()).then(|| VoiceConversation {
                            provider: package.clone(),
                            key: route.conversation,
                        }),
                    });
                    match chosen.entry(route.listener) {
                        std::collections::btree_map::Entry::Vacant(e) => {
                            e.insert((route.priority, playback.clone()));
                        }
                        std::collections::btree_map::Entry::Occupied(mut e) => {
                            if route.priority > e.get().0 {
                                e.insert((route.priority, playback.clone()));
                            } else if route.priority == e.get().0 && e.get().1 != playback {
                                e.insert((route.priority, None));
                                note(
                                    self.packages.as_mut().unwrap(),
                                    Diagnostic::error(
                                        "voice.conflict",
                                        "conflicting equal-priority voice routes; recipient denied",
                                    )
                                    .at(package),
                                );
                            }
                        }
                    }
                }
            }
            for (listener, (_, playback)) in chosen {
                if let Some(playback) = playback {
                    result
                        .entry(listener)
                        .or_insert_with(BTreeMap::new)
                        .insert(speaker, playback);
                }
            }
        }
        result
    }
    /// Speech originates at the body; moving a spectator camera cannot move it.
    pub fn voice_origin(&self, owner: OwnerId) -> Option<[f32; 3]> {
        self.peers.get(&owner).map(|p| p.player.eye().to_array())
    }
}
