# Docs

Choose a path below. The guides describe the public alpha; a source checkout
can contain changes newer than your download. For a particular build, use its
included guides and [release notes](https://github.com/MaxHastings/BlocklandReImagined/releases).

| I want to… | Start here |
|---|---|
| Install, update, bring old saves or play with friends | [Tester guide](TESTER-GUIDE.md) |
| Find controls or improve performance | [Playtest notes](PLAYTEST.md) |
| Check capabilities and unfinished behavior | [Features](FEATURES.md) and [known issues](KNOWN-ISSUES.md) |
| Run a server without the client | [Dedicated server](DEDICATED-SERVER.md) |
| Build games with brick events | [Rule Workshop](rule-workshop/PLAYTEST.md) |
| Make or import an Add-On | [Making Add-Ons](modding/README.md) |
| Build the engine or contribute | [Contributing](../CONTRIBUTING.md) |

The sections below are reference routes; you do not need to read them all to play.

## Playing and testing

Release-folder guides and platform notes:

- [Release notes](https://github.com/MaxHastings/BlocklandReImagined/releases): what each version changed and its known issues.
  Older focused checks are in [rule-workshop/](rule-workshop/).

- [TESTER-GUIDE.md](TESTER-GUIDE.md): install, playing together, what to send.
- [PLAYTEST-MAC.md](PLAYTEST-MAC.md): Mac startup and file locations; this
  additional guide ships with Mac releases.
- [FEATURES.md](FEATURES.md): what is done, partly done and missing.
- [PLAYTEST.md](PLAYTEST.md): things to try, default keys, slow-PC tips.
- [KNOWN-ISSUES.md](KNOWN-ISSUES.md): known problems.
- [DEDICATED-SERVER.md](DEDICATED-SERVER.md): headless hosting, persistence,
  admin settings and Add-Ons on Windows or Linux.
- [stress-lab/PLAYTEST-STRESS-LAB.md](stress-lab/PLAYTEST-STRESS-LAB.md):
  the Stress Lab game mode, in builds packaged with it.

## Building games without code

Start with the Wrench's Events screen and MiniGames for teams, equipment
and settings. These guides ship with every release under `RULE-WORKSHOP-*`
filenames.

- [rule-workshop/PLAYTEST.md](rule-workshop/PLAYTEST.md): editable examples,
  IF checks, regions, state and objects; the Workshop remains experimental.
- [rule-workshop/CREATOR-TEST-CARD.md](rule-workshop/CREATOR-TEST-CARD.md):
  short creator journeys and checks to try.
- [rule-workshop/DESIGN.md](rule-workshop/DESIGN.md): rule semantics and limits.

## Making Add-Ons

- [modding/README.md](modding/README.md): the guide. Making an Add-On,
  importing and porting v20 Add-Ons, and what players are asked to trust.
- [modding/ENGINE-CAPABILITIES.md](modding/ENGINE-CAPABILITIES.md): what
  the engine offers a new game, by what you want to build;
  [architecture/seams.md](architecture/seams.md) holds each seam's status.
- [audits/total-conversion.md](audits/total-conversion.md): the 2026-09-29
  audit behind the Commando sample. Historical; its "next" items are not a
  current list.
- [architecture/packages.md](architecture/packages.md): the Add-On format
  (`package.json`, `provides`, content kinds, client code).
- [architecture/package-runtime.md](architecture/package-runtime.md):
  how rules, scripts and hooks run on the host.
- [architecture/client-sandbox.md](architecture/client-sandbox.md): code
  on players' PCs, its host API, budgets and trust tiers.
- [architecture/mod-manager.md](architecture/mod-manager.md): the
  in-game Add-Ons screen, downloads on join and importing.
- [modding/porting.md](modding/porting.md): porting the behaviour of a v20
  Add-On's scripts natively.
- [modding/maps.md](modding/maps.md): Blender maps and legacy map imports.
- [modding/gltf-models.md](modding/gltf-models.md): bodies, held models,
  vehicles and bricks from Blender/glTF.

## Working on the game

Read [AGENTS.md](../AGENTS.md) first, then:

- [STATUS.md](STATUS.md): decisions already made, release state, what is
  open.
- [alpha-contract.md](alpha-contract.md): the scope and acceptance items.
  [playtest-contract.md](playtest-contract.md) is the earlier playtest gate,
  kept as history.
- [architecture/platform-principles.md](architecture/platform-principles.md):
  engine owns mechanisms, Add-Ons own policy; read before changing
  identity, saves, the wire protocol or Add-Ons.
- [architecture/seams.md](architecture/seams.md): which thread owns each
  engine seam being built; check it before starting one.
- [progress.md](progress.md): dated decisions, evidence and commands up to
  2026-10-01; newer entries are one file each in [progress/](progress/README.md).
- [content-regeneration.md](content-regeneration.md): what
  `tools/bootstrap.py` does, step by step.
- [testing-tiers.md](testing-tiers.md): quick, focused and integrated checks.
- [playtest-package-layout.md](playtest-package-layout.md): building and
  checking a release folder.
- [release-builds.md](release-builds.md): the GitHub Actions release build,
  its one-time content setup, and tagging a release.
- [vanilla-reference.md](vanilla-reference.md): the v20 install used as
  the reference, and coverage of its content.

### Systems

These implementation notes include dated checkpoints, commands and test evidence.
Old schema/protocol numbers describe those checkpoints; current constants and
tests in source define the running formats. Use the modding guides above for
supported authoring workflows, and STATUS for current release state.

- [world-state.md](world-state.md): authoritative state, events, saves.
- [networking.md](networking.md) and
  [architecture/hosting.md](architecture/hosting.md): the protocol, host,
  replication and direct-IP hosting.
- [player-simulation.md](player-simulation.md): movement, sessions and
  players. [physics-decision.md](physics-decision.md): why Rapier.
- [building-simulation.md](building-simulation.md) and
  [brick-materials.md](brick-materials.md): bricks, the catalog and prints.
- [native-client.md](native-client.md): the client, renderer and input.
- [../crates/events/README.md](../crates/events/README.md): the current event
  runtime, timing, targets and limits. [rule-workshop/DESIGN.md](rule-workshop/DESIGN.md)
  covers creator-rule semantics. [event-modernization.md](event-modernization.md)
  records the original modernization requirements and implementation checkpoint.
- [architecture/bots.md](architecture/bots.md): current bot behavior, bounded
  objectives and verification limits. [architecture/bot-interactions.md](architecture/bot-interactions.md)
  preserves the v0.2.0 interaction design and evidence.
- [avatar-pipeline.md](avatar-pipeline.md): the Blockhead rig and
  animation.
- Runtime pieces: [audio](runtime-audio.md), [effects](runtime-effects.md),
  [foliage](runtime-foliage.md), [inventory](runtime-inventory.md),
  [weapons](runtime-weapons-host.md), [weather](runtime-weather.md),
  [world items](runtime-world-items.md),
  [water and weather](map-water-weather.md),
  [sky and fog](environment-pipeline.md).
- Conversion from v20: [content](content-conversion.md),
  [effects](effects-conversion.md), [UI](ui-conversion.md).
- [crash-hunt.md](crash-hunt.md): fuzzers and chaos soaks.
- Some crates have a `README.md` describing their API; the rest are
  documented in their source.

### Audits and research

[audits/](audits) holds dated reviews: v20 parity, fidelity and
behaviour, first impressions, red team, bandwidth and net graph,
orthogonality and more. Read [audits/bug-patterns.md](audits/bug-patterns.md)
before fixing a bug.
Each records what it found at its checkpoint; open findings may have later
follow-ups. Release-readiness pages, `v022-delivery-contract.md`,
`v023-work-plan.md`, dated Rule Workshop checks and the first-playtest contract
are historical release plans. They do not reopen published releases or prove
current acceptance. [research/](research)
holds the evidence gathered from v20's files while each system was built.
[stress-lab/](stress-lab) holds the Stress Lab's handoff and weakness
ledger. They are history: when one disagrees with the code, the code wins.
