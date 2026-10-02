#!/usr/bin/env python3
"""Group App's fields into per-system structs, mechanically.

Run after split_app.py. Each system struct lives in its module and owns its
fields; App holds one field per system. `self.field` becomes
`self.<system>.field`; the remaining accesses are fixed from the compiler's
E0609/E0560/E0063 errors until the crate checks clean.

Usage: group_fields.py <repo root>
"""
import json
import os
import re
import subprocess
import sys

ROOT = sys.argv[1]
APP = os.path.join(ROOT, "crates/client/src/app")

# (App field, struct, module, doc, fields)
SYSTEMS = [
    ("net", "SessionState", "session",
     "The connection to a game: the attempt in flight, its pending requests and what was last sent.",
     "attempt steering_sent abilities brick_hand pending_actions reconnects invite join_notices environment_sent dialog_epoch trigger_epoch"),
    ("scene", "SceneState", "scene",
     "The world as the CPU sees it: map scene, brick chunks, the world log and the query mirror.",
     "cpu_scene cpu_terrain chunked cpu_chunks cpu_chunk_bricks chunk_hides chunks_left_out liquid_cache world_source world_revision world_log world_job query_source query_log scene_map meshes mirror_shapes mirror_index palette materials shape_indices"),
    ("gpu", "GpuState", "gpu",
     "Everything uploaded to the graphics card, and the renderers that draw it.",
     "renderer gpu_scene gpu_terrain gpu_palette gpu_chunks gpu_chunk_bricks chunk_uploads depth gpu_broken gpu_restart gpu_name gpu_passes time_passes hidden_lines vignette selection_lines selection_uploaded hidden_uploaded hidden_fading weather_renderer effects_renderer shell_gpu ghost_gpu ghost_uploaded ghost_look"),
    ("lighting", "Lighting", "lighting",
     "Map lighting: the baked light volume, reflections and the environment probe.",
     "light_volume reflections environment_probe"),
    ("view", "ViewState", "view",
     "What the camera shows beyond the controls: observer and rendered eyes, the drawn controls, crosshair and wheels.",
     "crosshair_hidden tool_wheel camera_wheel aim_wheel scope_overlay observer_eye rendered_camera rendered_roll drawn_controls"),
    ("mounts", "Mounts", "mounts",
     "Seats and riders: what the local player sits on and how riders are posed.",
     "mount_heading seated_on takes_turret seat_report rider_rotations rider_eye tumble"),
    ("fx", "Effects", "fx",
     "Presentation effects: weapon, actor and world effects, debris, fades and the cue queues feeding them.",
     "effects weapon_effects actor_effects explosion_shapes beams explosion_debris weapon_shells weapon_cues weapon_cue_drops brick_debris debris_models brick_fades fade_models brick_kills weapon_light_deferred weapon_effect_session weapon_animation_cues weapon_animation_drops weapon_animation_cursor tutorial_targets"),
    ("avatar", "Avatars", "avatars",
     "Avatar bodies, their actions and gestures, and the avatar screen's preview.",
     "avatar_assets avatars mount_meshes avatar_actions avatar_threads avatar_action_images animation_time avatar_preview preview_request preview_dirty"),
    ("addons", "AddOns", "addons",
     "Add-On packages: the catalog, client code, server packages and imports.",
     "package_catalog client_code item_skins server_packages packages_from_tools skip_add_on_reload package_models add_on_sync add_on_health left_out_add_ons"),
    ("files", "Saves", "saves",
     "Saves and their pictures, file jobs, old saves and colour-set loads.",
     "save_pictures save_previews save_picture save_shots saves file_jobs old_saves old_saves_started save_refresh color_load"),
    ("lobby", "Lobby", "lobby",
     "Finding games: LAN hosts and queries, the update check and the firewall fix.",
     "update_check lan_query firewall_fix lan_hosts"),
    ("perf", "Perf", "perf",
     "Performance: frame stats and log, network sampling, lag watch and quality.",
     "net_sampler lag_watch perf_stats_due frame_stats frame_log auto_quality frame_limit"),
    ("build", "BuildState", "building",
     "Building: the brick hand and ghosts, tool dialogs and build macros.",
     "building ghost_report remote_ghosts tool_ui tool_takes_paint macro_recording build_macro macro_playback"),
]
FIELD_SYSTEM = {}
for sysfield, struct, module, doc, fields in SYSTEMS:
    for f in fields.split():
        assert f not in FIELD_SYSTEM, f
        FIELD_SYSTEM[f] = sysfield
SYS = {s[0]: s for s in SYSTEMS}


def read(p):
    return open(p).read()


def write(p, t):
    open(p, "w").write(t)


def balance(s):
    s = s.replace("->", "")
    return s.count("(") + s.count("[") + s.count("<") + s.count("{") - s.count(")") - s.count("]") - s.count(">") - s.count("}")


# 1. Cut the fields out of `pub struct App`.
mod_path = os.path.join(APP, "mod.rs")
mod = read(mod_path)
start = mod.index("pub struct App {\n")
end = mod.index("\n}\n", start)
body = mod[start + len("pub struct App {\n"):end].split("\n")
entries = []  # (lines, name)
pending = []
i = 0
while i < len(body):
    line = body[i]
    s = line.strip()
    if s.startswith("///") or s.startswith("//") or s.startswith("#["):
        pending.append(line)
        i += 1
        continue
    m = re.match(r"^    (?:pub(?:\([a-z]+\))? )?([a-z_][a-z0-9_]*): ", line)
    assert m, line
    lines = [line]
    b = balance(line.split(":", 1)[1])
    while not (b == 0 and lines[-1].rstrip().endswith(",")):
        i += 1
        lines.append(body[i])
        b += balance(body[i])
    entries.append((pending + lines, m.group(1), line.lstrip().startswith("pub")))
    pending = []
    i += 1
assert not pending
moved = {}
kept = []
placed = set()
for lines, name, public in entries:
    sysfield = FIELD_SYSTEM.get(name)
    if sysfield and not public:
        moved.setdefault(sysfield, []).append(lines)
        if sysfield not in placed:
            placed.add(sysfield)
            _, struct, module, doc, _ = SYS[sysfield]
            kept.append([f"    /// {doc}", f"    {sysfield}: {struct},"])
    else:
        assert not sysfield, f"{name} is public"
        kept.append(lines)
missing = set(FIELD_SYSTEM) - {n for _, n, _ in entries}
assert not missing, missing
mod = mod[:start] + "pub struct App {\n" + "\n".join(l for ls in kept for l in ls) + mod[end:]

# 2. Each system's struct, in its module.
for sysfield, struct, module, doc, fields in SYSTEMS:
    path = os.path.join(APP, f"{module}.rs")
    if os.path.exists(path):
        text = read(path)
    else:
        text = f"//! {doc}\nuse super::*;\n"
        decl_at = mod.index("\nmod ") + 1
        mod = mod[:decl_at] + f"mod {module};\n" + mod[decl_at:]
    out = [f"/// {doc}", f"pub(super) struct {struct} {{"]
    for lines in moved[sysfield]:
        for l in lines:
            out.append(re.sub(r"^    ([a-z_][a-z0-9_]*): ", r"    pub(super) \1: ", l))
    out.append("}")
    at = text.index("use super::*;\n") + len("use super::*;\n")
    text = text[:at] + "\n" + "\n".join(out) + "\n" + text[at:]
    write(path, text)
    use = f"use {module}::*;"
    if use not in mod:
        struct_at = mod.index("pub struct App {")
        last_mod = max(m.end() for m in re.finditer(r"(?m)^mod [a-z_]+;\n", mod[:struct_at]))
        mod = mod[:last_mod] + use + "\n" + mod[last_mod:]
write(mod_path, mod)

# 3. `self.field` -> `self.<system>.field` everywhere in app/.
pat = re.compile(r"\bself(\s*)\.(" + "|".join(sorted(FIELD_SYSTEM, key=len, reverse=True)) + r")\b(?!\s*\()")
for f in os.listdir(APP):
    p = os.path.join(APP, f)
    t = read(p)
    t2 = pat.sub(lambda m: f"self{m.group(1)}.{FIELD_SYSTEM[m.group(2)]}.{m.group(2)}", t)
    if t2 != t:
        write(p, t2)


# 4. The App literal in load.rs: regroup its fields.
def regroup_literal(path, opener):
    t = read(path)
    at = t.index(opener) + len(opener)
    depth = 0
    j = at
    # Find the literal's closing brace.
    while True:
        c = t[j]
        if c in "([{":
            depth += 1
        elif c in ")]}":
            if depth == 0:
                break
            depth -= 1
        j += 1
    inner = t[at:j]
    # Split entries at depth-0 commas, keeping comments with the next entry.
    parts = []
    depth = 0
    cur = ""
    k = 0
    in_str = False
    while k < len(inner):
        c = inner[k]
        if in_str:
            cur += c
            if c == "\\":
                cur += inner[k + 1]
                k += 2
                continue
            if c == '"':
                in_str = False
            k += 1
            continue
        if c == '"':
            in_str = True
        elif inner.startswith("//", k):
            e = inner.index("\n", k)
            cur += inner[k:e]
            k = e
            continue
        elif c in "([{":
            depth += 1
        elif c in ")]}":
            depth -= 1
        if c == "," and depth == 0:
            parts.append(cur)
            cur = ""
        else:
            cur += c
        k += 1
    tail = cur
    assert tail.strip() == "", tail
    indent = re.search(r"\n(\s*)\S", inner).group(1)
    groups = {}
    order = []
    for p in parts:
        name = re.search(r"^\s*(?://[^\n]*\n\s*)*([a-z_][a-z0-9_]*)\s*(?::|$)", p).group(1)
        sysfield = FIELD_SYSTEM.get(name)
        if sysfield:
            if sysfield not in groups:
                groups[sysfield] = []
                order.append(("sys", sysfield))
            groups[sysfield].append(p)
        else:
            order.append(("field", p))
    out = []
    for kind, v in order:
        if kind == "field":
            out.append(v + ",")
        else:
            struct = SYS[v][1]
            sub = "".join(p.replace("\n", "\n    ") + "," for p in groups[v])
            out.append(f"\n{indent}{v}: {struct} {{{sub}\n{indent}}},")
    t = t[:at] + "".join(out) + tail + t[j:]
    write(path, t)


regroup_literal(os.path.join(APP, "load.rs"), "let mut app = Self {")


# 5. Fix the remaining accesses from compiler errors.
def check():
    r = subprocess.run(
        ["cargo", "check", "-p", "bri-client", "--all-targets", "--message-format=json"],
        cwd=ROOT, capture_output=True, text=True,
    )
    errs = []
    for line in r.stdout.splitlines():
        try:
            m = json.loads(line)
        except ValueError:
            continue
        if m.get("reason") != "compiler-message":
            continue
        msg = m["message"]
        if msg["level"] != "error":
            continue
        errs.append(msg)
    return errs


for round_ in range(30):
    errs = check()
    if not errs:
        print("clean after", round_, "fix rounds")
        break
    fixes = {}
    other = []
    for e in errs:
        code = (e.get("code") or {}).get("code")
        mm = re.match(r"(?:no field|attempted to take value of method) `([a-z_0-9]+)` on type", e["message"])
        if code in ("E0609", "E0615") and mm and mm.group(1) in FIELD_SYSTEM:
            sp = [s for s in e["spans"] if s["is_primary"]][0]
            fixes.setdefault(sp["file_name"], set()).add((sp["line_start"], sp["column_start"], mm.group(1)))
        else:
            other.append(e)
    if not fixes:
        for e in other[:20]:
            sp = [s for s in e["spans"] if s["is_primary"]]
            loc = f"{sp[0]['file_name']}:{sp[0]['line_start']}" if sp else ""
            print("ERROR", loc, e["message"])
        sys.exit(1)
    for fname, locs in fixes.items():
        p = os.path.join(ROOT, fname)
        ls = read(p).split("\n")
        for line, col, field in sorted(locs, reverse=True):
            l = ls[line - 1]
            c = col - 1
            assert l[c:c + len(field)] == field, (fname, line, l, field)
            ls[line - 1] = l[:c] + FIELD_SYSTEM[field] + "." + l[c:]
        write(p, "\n".join(ls))
else:
    print("did not converge")
    sys.exit(1)
