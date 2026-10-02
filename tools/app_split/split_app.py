#!/usr/bin/env python3
"""Mechanically split crates/client/src/app.rs into crates/client/src/app/.

Moves `impl App` methods and a few self-contained types into child modules
by name, without changing any code inside them. Re-runnable on a newer main:
names it does not know stay in app/mod.rs.

Usage: split_app.py <repo root>
"""
import os
import re
import sys

ROOT = sys.argv[1]
SRC = os.path.join(ROOT, "crates/client/src/app.rs")
OUT = os.path.join(ROOT, "crates/client/src/app")

# Inherent `impl App` methods, by the module that owns them.
METHODS = {
    "addons": "sync_add_ons check_add_ons show_add_ons add_on_health host_notice enable_packages add_ons_changed apply_packages notify_left_out_add_ons update_package_hud",
    "fx": "queue_cue_heard_at pending_kills queue_weapon_cue update_actor_effects reset_weapon_effect_session update_weapon_effects update_weapon_effect_parts",
    "avatars": "update_avatar_animation_inputs",
    "probes": "drawn_vehicle host_on_any_port hosted_port loading_revision item_assets add_on_code_running weapon_shell_count world_item_stats world_item_instances foliage_stats foliage_placement weather_counts weather_diagnostics render_stats time_gpu_passes gpu_pass_times frame_stats pick_quality player_session audio_stats audio_requests audio_warnings effect_counts weapon_effect_counts weapon_effect_diagnostics weapon_effect_backlog avatar_scene avatar_body avatar_action avatar_node rendered_camera rendered_roll held_image_transform building pending_requests world_render_ready local_motion network_view entity_counts scene_map presented_local",
    "load": "load load_with_audio",
    "session": "answer handle_admin disconnect join_name prompt_for_name send_name host identity_question forget_server_identity join command accept_reply show_progress map_setup_updates",
    "view": "view_kick spectating local_alive chase_camera player_camera local_eye pushers rider_eye view_camera view_camera_here follow_control camera_view update_held_weapon third_person_view local_weapon_seat",
    "mounts": "predict_driven pose_mounts",
    "perf": "update_lag update_perf",
    "hud": "update_combat_presentation",
    "building": "invalidate_tool_dialogs handle_building",
    "saves": "poll_files take_save_picture show_save_files start_old_saves poll_old_saves send_load choose_color_load colorset track_unsaved",
    "net_events": "poll_network",
}
# `PlatformApp` methods whose bodies move into an inherent method.
DELEGATED = {"tick": ("frame", "frame"), "pump": ("actions", "dispatch"), "render_scene": ("render", "render_frame")}
# Top-level items (by name) that move whole.
ITEMS = {
    "lighting": "Baked LightVolumeState map_light_tints store_bake",
    "hud": "CombatPresentation",
}
DOCS = {
    "addons": "Add-On packages: enabling, applying and their HUD.",
    "fx": "Weapon, actor and world effects fed by the session's cues.",
    "avatars": "Avatar animation inputs.",
    "probes": "Read-only views of the game for tests, tools and the console.",
    "load": "Building the App: content, audio and every system's start state.",
    "session": "Hosting, joining, commands and their replies.",
    "view": "The camera: whose eyes, which mode, where it looks.",
    "mounts": "Seats and riders: driven-vehicle prediction and mount poses.",
    "perf": "Lag and frame performance sampling.",
    "hud": "Combat presentation: hugs, hidden bodies, hit feedback.",
    "building": "Building tools and their dialogs.",
    "saves": "Saves, save pictures, old saves and colour sets.",
    "net_events": "Draining the network worker's events each frame.",
    "frame": "The per-frame update, in its fixed order (see docs/architecture/client-app-split.md).",
    "actions": "Dispatching UI actions after each input event.",
    "render": "Rendering the scene each frame.",
    "lighting": "Baked map lighting: the light volume and its cache.",
}

text = open(SRC).read()
lines = text.split("\n")


def depths(lines):
    """Brace depth at the start of each line, skipping strings and comments."""
    out = []
    depth = 0
    in_block = 0
    raw_end = None
    in_str = False
    for line in lines:
        out.append(depth)
        i = 0
        n = len(line)
        while i < n:
            c = line[i]
            if raw_end is not None:
                if line.startswith(raw_end, i):
                    i += len(raw_end)
                    raw_end = None
                else:
                    i += 1
                continue
            if in_str:
                if c == "\\":
                    i += 2
                    continue
                if c == '"':
                    in_str = False
                i += 1
                continue
            if in_block:
                if line.startswith("*/", i):
                    in_block -= 1
                    i += 2
                elif line.startswith("/*", i):
                    in_block += 1
                    i += 2
                else:
                    i += 1
                continue
            if line.startswith("//", i):
                break
            if line.startswith("/*", i):
                in_block += 1
                i += 2
                continue
            m = re.match(r'b?r(#*)"', line[i:])
            if m and (i == 0 or not (line[i - 1].isalnum() or line[i - 1] == "_")):
                raw_end = '"' + m.group(1)
                i += m.end()
                continue
            if c == '"':
                in_str = True
                i += 1
                continue
            if c == "'":
                m = re.match(r"'(\\(x[0-9a-fA-F]{2}|u\{[0-9a-fA-F]+\}|.)|[^\\'])'", line[i:])
                if m:
                    i += m.end()
                    continue
                i += 1
                continue
            if c == "{":
                depth += 1
            elif c == "}":
                depth -= 1
            i += 1
    assert depth == 0 and not in_str and raw_end is None and not in_block, "unbalanced"
    return out


D = depths(lines)


def lead(i, floor):
    """Start of the item at line i, with its doc comments and attributes."""
    j = i
    while j > floor:
        s = lines[j - 1].strip()
        if s.startswith("///") or s.startswith("#[") or s.startswith("//") and not s.startswith("////"):
            j -= 1
        else:
            break
    return j


name_re = re.compile(r"^\s*(?:pub(?:\([a-z]+\))? )?(?:async )?(?:const )?(?:unsafe )?(fn|struct|enum|impl|mod|const|type|static|trait)\b\s*(?:<[^>]*>\s*)?([A-Za-z_][A-Za-z0-9_]*)?(?:\s+for\s+([A-Za-z_][A-Za-z0-9_]*))?")

# Top-level items.
items = []  # (start, end, kind, name)
i = 0
while i < len(lines):
    if D[i] == 0 and lines[i].strip() and not lines[i].startswith((" ", "}", "/", "#")):
        m = name_re.match(lines[i])
        start = lead(i, items[-1][1] + 1 if items else 0)
        if m:
            kind, name = m.group(1), m.group(2)
            if kind == "impl" and m.group(3):
                name = f"{name} for {m.group(3)}"
        else:
            kind, name = "other", None
        j = i
        while not (D[j + 1] == 0 and lines[j].rstrip().endswith(("}", ";")) if j + 1 < len(lines) else True):
            j += 1
        items.append((start, j, kind, name))
        i = j + 1
    else:
        i += 1

body_of = {}
for s, e, k, n in items:
    body_of.setdefault((k, n), []).append((s, e))

# Methods inside every `impl App` block (depth 1).
methods = []  # (start, end, name, impl_start)
for s, e, k, n in items:
    if k == "impl" and n in ("App", "PlatformApp for App"):
        i = s
        while i <= e:
            if D[i] == 1 and re.match(r"^    (pub(\([a-z]+\))? )?(async )?fn ", lines[i]):
                start = lead(i, methods[-1][1] + 1 if methods and methods[-1][3] == s else s + 1)
                j = i
                while not (D[j + 1] == 1 and lines[j].rstrip().endswith("}")):
                    j += 1
                name = re.match(r"^    (?:pub(?:\([a-z]+\))? )?(?:async )?fn ([a-z_0-9]+)", lines[i]).group(1)
                methods.append((start, j, name, s, n))
                i = j + 1
            else:
                i += 1

method_module = {}
for mod, names in METHODS.items():
    for name in names.split():
        method_module[name] = mod
item_module = {}
for mod, names in ITEMS.items():
    for name in names.split():
        item_module[name] = mod

files = {}  # module -> list of chunks
moved = set()  # line indices removed from mod.rs
replacements = {}  # start line -> replacement lines (for delegated)


def private_to_super(chunk):
    out = []
    for line in chunk:
        out.append(re.sub(r"^(\s*)(async |const |unsafe )?fn ", lambda m: f"{m.group(1)}pub(super) {m.group(2) or ''}fn ", line, count=1) if re.match(r"^\s*(async |const |unsafe )?fn ", line) else line)
    return out


for start, end, name, impl_start, impl_name in methods:
    chunk = lines[start:end + 1]
    if impl_name == "App" and name in method_module:
        mod = method_module[name]
        files.setdefault(mod, []).append(("method", private_to_super(chunk)))
        moved.update(range(start, end + 1))
    elif impl_name == "PlatformApp for App" and name in DELEGATED:
        mod, new = DELEGATED[name]
        header_end = start
        while not lines[header_end].rstrip().endswith("{"):
            header_end += 1
        header = lines[start:header_end + 1]
        fn_line = next(k for k in range(start, header_end + 1) if re.match(r"^    fn ", lines[k]))
        sig = "\n".join(lines[fn_line:header_end + 1])
        params = re.search(r"\(&mut self(?:, ([^)]*))?\)", sig)
        args = ", ".join(p.split(":")[0].strip() for p in (params.group(1) or "").split(",") if p.strip())
        moved_header = [re.sub(r"^    fn " + name + r"\b", f"    pub(super) fn {new}", l) for l in header]
        files.setdefault(mod, []).append(("method", moved_header + lines[header_end + 1:end + 1]))
        keep = lines[start:header_end + 1] + [f"        self.{new}({args})", "    }"]
        replacements[start] = keep
        moved.update(range(start, end + 1))

for s, e, k, n in items:
    base = n.split(" for ")[-1] if n and k == "impl" else n
    if base in item_module and (k != "impl" or " for " not in (n or "")):
        mod = item_module[base]
        chunk = lines[s:e + 1]
        if k in ("struct", "enum"):
            chunk = [re.sub(r"^(struct|enum) ", r"pub(super) \1 ", l) for l in chunk]
            if k == "struct":
                chunk = [re.sub(r"^    ([a-z_][a-z0-9_]*): ", r"    pub(super) \1: ", l) for l in chunk]
        else:
            chunk = private_to_super(chunk)
        files.setdefault(mod, []).append(("item", chunk))
        moved.update(range(s, e + 1))
    elif k == "mod" and n == "tests":
        chunk = lines[s:e + 1]
        # Body of `mod tests { ... }` becomes app/tests.rs.
        open_line = next(x for x in range(s, e + 1) if lines[x].startswith("mod tests"))
        files["tests"] = [("raw", [l[4:] if l.startswith("    ") else l for l in lines[open_line + 1:e]])]
        test_attr = lines[s:open_line]
        replacements[s] = test_attr + ["mod tests;"]
        moved.update(range(s, e + 1))

# Write mod.rs.
mod_lines = []
i = 0
while i < len(lines):
    if i in replacements:
        mod_lines.extend(replacements[i])
    if i not in moved:
        mod_lines.append(lines[i])
    i += 1
mod_text = "\n".join(mod_lines)
# Drop impl blocks left empty.
mod_text = re.sub(r"\nimpl App \{\n\}\n", "\n", mod_text)
modules = sorted(m for m in files if m != "tests")
decl = "\n".join(f"mod {m};" for m in modules)
uses = "\n".join(f"use {m}::*;" for m in sorted(set(item_module.values()) & set(modules)))
# Declare the modules after the crate-level doc comment and imports.
first_item = re.search(r"\n(type |const |struct |pub |enum |fn )", mod_text)
mod_text = mod_text[: first_item.start()] + "\n" + decl + "\n" + uses + mod_text[first_item.start():]
os.makedirs(OUT, exist_ok=True)
open(os.path.join(OUT, "mod.rs"), "w").write(mod_text)
for m, chunks in files.items():
    body = []
    if m == "tests":
        body = chunks[0][1]
        open(os.path.join(OUT, "tests.rs"), "w").write("\n".join(body).rstrip("\n") + "\n")
        continue
    body.append(f"//! {DOCS.get(m, m)}")
    body.append("use super::*;")
    body.append("")
    meth = [c for k, c in chunks if k == "method"]
    rest = [c for k, c in chunks if k == "item"]
    for c in rest:
        body.extend(c)
    if meth:
        body.append("impl App {")
        for c in meth:
            body.extend(c)
        body.append("}")
    open(os.path.join(OUT, f"{m}.rs"), "w").write("\n".join(body).rstrip("\n") + "\n")
os.remove(SRC)
print("modules:", ", ".join(sorted(files)))
print("mod.rs lines:", len(mod_text.split("\n")))
unknown = [n for s, e, n, i, imp in methods if imp == "App" and n not in method_module]
print("methods left in mod.rs:", " ".join(unknown) or "none")
