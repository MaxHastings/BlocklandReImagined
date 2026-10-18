# Contributing

Start with [AGENTS.md](AGENTS.md) for repository rules, then
[STATUS.md](docs/STATUS.md) for release state and decisions. The
[alpha contract](docs/alpha-contract.md) is the broader roadmap; a published
alpha does not mean every acceptance item is complete.

## Choose a starting point

- **Report a bug:** follow [the tester guide](docs/TESTER-GUIDE.md#when-something-breaks).
  Include the menu version/commit, steps, expected behavior, actual behavior
  and relevant logs. For performance, include the map, build size, hardware
  and graphics settings. Keep identity files private.
- **Make an Add-On:** use [the modding guide](docs/modding/README.md).
  Release tools can check and run rules without Rust or a source build.
- **Change the engine:** use [README setup](README.md#develop) and
  [content regeneration](docs/content-regeneration.md). Source installations
  stay read-only; generated content and recovered originals stay out of Git.
- **Find a subsystem:** use the [documentation index](docs/README.md).
  Check [seam ownership](docs/architecture/seams.md) before building a shared
  mechanism. The engine owns mechanisms; Add-Ons own game policy.

## Validate a change

Run commands from the repository root with Python 3.11+ (`python3` on systems
where that is its name). Pick the tier that covers the change:

```sh
python tools/check.py quick
python tools/check.py focused
```

Quick covers changed crates; focused also covers their dependents. Use
`python tools/check.py plan focused` to inspect the plan. Run the full gate
once per integrated candidate, through the coordinator; see
[test tiers](docs/testing-tiers.md). Documentation-only work also runs:

```sh
python tools/check_docs.py
python -m unittest discover -s tools -p "test_*docs*.py"
```

Maxwell performs interactive playtests. Use headless tests, logs and offscreen
rendering; do not automate gameplay or launch a visible game for routine checks.

Keep changes focused, record meaningful evidence and remaining limits in a new
[progress entry](docs/progress/README.md), and open a PR with the problem,
resulting behavior and validation. PRs need the required CI checks. Main
integration uses `python tools/gate.py --push`; never bypass the hook or
force-push main. Install the hook with `python tools/gate.py --install-hook`.

## Keep documentation with the change

- Update the relevant player or creator guide when behavior, controls,
  requirements, paths or commands change. Update known issues when a
  demonstrated fix closes a report; retain unconfirmed reports.
- Describe functionality users can reach through ordinary play. Label partial,
  experimental and unsupported behavior. API support and an isolated test do
  not establish a complete game or source-fidelity claim.
- Put current release state in `docs/STATUS.md`, per-version changes in
  `docs/releases/`, and investigation evidence in `docs/progress/`. Link to
  these rather than repeating mutable version/candidate lists everywhere.
- Keep dated audits, research and release checklists as history. Add a clear
  historical label and a route to current guidance when they are easy to mistake
  for today's instructions. Do not rewrite old evidence as a current result.
- Guides ship outside the repository too. Check local links both in source
  and in the flat release layout; `tools/package_guides.py` rewrites them.
  `check_docs.py` checks those layouts without game content or a build.
