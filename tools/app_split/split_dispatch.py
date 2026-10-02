#!/usr/bin/env python3
"""Split dispatch's per-action loop body into intercept_action and
dispatch_action, unchanged apart from `continue` becoming an early return.

Usage: split_dispatch.py <repo root>
"""
import os, re, sys
p = os.path.join(sys.argv[1], "crates/client/src/app/actions.rs")
L = open(p).read().split("\n")
loop = L.index("        for (id, action) in self.ui.drain_actions() {")
match = L.index("            let result = match action {")
answer = L.index("            self.answer(id, result);", match)
dedent = lambda ls: [l[4:] if l.strip() else l for l in ls]
def returns(ls, what):
    return [re.sub(r"\bcontinue;", what + ";", l).replace("=> continue,", "=> " + what + ",") for l in ls]
pre = returns(dedent(L[loop + 1 : match]), "return true")
rest = returns(dedent(L[match : answer + 1]), "return Ok(())")
# The intercept borrows the action the original consumed.
fixes = [
    ("let command = if down {", "let command = if *down {"),
    ("} else if down && matches!", "} else if *down && matches!"),
    ("Command::WeaponTrigger { down }", "Command::WeaponTrigger { down: *down }"),
    ("UiAction::Admin(action) => action,", "UiAction::Admin(action) => action.clone(),"),
    ("(&action)", "(action)"),
    ("(&action, ", "(action, "),
    (", &action)", ", action)"),
]
for old, new in fixes:
    pre = [l.replace(old, new) for l in pre]
out = L[: loop + 1] + [
    "            if self.intercept_action(id, &action) {",
    "                continue;",
    "            }",
    "            self.dispatch_action(id, action, &mut platform)?;",
] + L[answer + 1 :]
end = max(i for i, l in enumerate(out) if l == "}")
out[end:end] = ["",
    "    /// Actions that never reach `dispatch_action`: a spectator's buttons,",
    "    /// a dead player's clicks, recorded macros and building actions.",
    "    /// True when `action` was handled here.",
    "    fn intercept_action(&mut self, id: RequestId, action: &UiAction) -> bool {",
] + pre + ["        false", "    }", "",
    "    /// Runs one UI action and answers it, or queues the platform command",
    "    /// it asks for.",
    "    fn dispatch_action(",
    "        &mut self,",
    "        id: RequestId,",
    "        action: UiAction,",
    "        platform: &mut Vec<PlatformCommand>,",
    "    ) -> Result<()> {",
] + rest + ["        Ok(())", "    }"]
open(p, "w").write("\n".join(out))
