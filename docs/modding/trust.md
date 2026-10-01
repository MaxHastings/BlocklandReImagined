# What players are asked to trust

Players download a server's Add-Ons when they join. What they are asked
depends on the most powerful thing an Add-On does:

| Tier | What the Add-On has | What the player sees |
|---|---|---|
| Data | rules, HUD panels, weapons, bricks, models, sounds | nothing: it downloads and runs |
| Sandboxed code | a `client` section, or skins in `looks.json`: WebAssembly and WGSL run in the sandbox | "Trust and join" or "Leave", once per server, and again when the code changes; nothing when the player installed the same code themselves |
| Elevated code | `net.http` or `files.addon_folder` | a separate, stronger prompt per Add-On (not offered to joiners yet, see [Still being built](README.md#still-being-built)) |

Rules always run on the host, never on players' PCs, so they need no
trust. Players can take any trust back with **Forget Trust** on the
Add-Ons screen. Ask for the smallest tier that does the job: most Add-Ons
are data only. The details are in
[client-sandbox.md](../architecture/client-sandbox.md).
