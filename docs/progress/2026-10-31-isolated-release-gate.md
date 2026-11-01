# Isolated release gate content

The standard extended gate fingerprints and mounts the main checkout's content,
then its startup check can install default Add-Ons there. The main checkout has
unrelated tracked edits and its content is not the exact hybrid release
candidate. Added an explicit `BRI_GATE_CONTENT` override so the gate can
fingerprint, mount and test an isolated candidate content copy instead. With no
override its existing main-checkout behavior is unchanged.

A reused gate worktree is refused if its existing content link points elsewhere;
the tool does not remove or retarget it. An explicit isolated gate also refuses
an inconsistent `BRI_CONTENT`, preventing tests and the recorded content from
silently diverging. `BRI_GATE_DIR` can place this gate under the approved
`BlocklandReImagined-worktrees` directory. This mechanism does not bypass any
test, performance budget, history or content check.

`python -m unittest discover -s tools -p test_gate.py` passed 55 tests,
including the isolated selection and disagreement rejection. The output contains
intentional mock examples of flaky and known-failure reporting, not actual test
failures. Documentation checks and `git diff --check` passed. The full release
gate has not yet run with the isolated candidate.
