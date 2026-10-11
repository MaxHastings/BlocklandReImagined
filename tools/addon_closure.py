#!/usr/bin/env python3
"""Dependency closure over an imported weapons Add-On.

A framework such as Rallypack defines dozens of weapons that share projectiles,
explosions, emitters, sounds, damage types, models and textures. Asking for a
few of its items should keep exactly what those items need and nothing else,
and say what they need that the Add-On does not define.

The closure is read from the data alone. It never runs a script and never looks
at a name for meaning: a record is needed because some string in a needed
record names it (`weapon_x:projectile/shot`, a sound profile, an emitter, a
damage type), or because it is the model or texture a needed model or image
points at.

    python tools/addon_closure.py PACKAGE_DIR ITEM... [--apply] [--json]

Without --apply nothing is changed: it prints what would be kept, dropped and
left dangling. With --apply it rewrites assets/weapons.json, presentation.json
and item-physics.json to the subset (recomputing their hashes) and deletes the
model, texture and sound files only the dropped records owned.
"""
import argparse
import hashlib
import json
import pathlib
import re
import sys

# Tables whose members are named by a bare symbol (`R_ShellDebris`,
# `$DamageType::Name`) as well as by their full id.
SYMBOLIC = ('definitions', 'sounds', 'damage_types', 'explosions',
            'fx.particles', 'fx.emitters', 'fx.lights', 'fx.explosions')
# Presentation tables that mirror a weapons table by key.
MIRRORED = ('items', 'images', 'projectiles')
# What a reference to another record looks like when it is namespaced.
NAMESPACED = re.compile(r'^[a-z0-9_\-]+:[a-z_]+/[^\s]+$')


def read(path):
    return json.loads(pathlib.Path(path).read_text(encoding='utf-8'))


def write(path, value):
    pathlib.Path(path).write_text(json.dumps(value, indent=2) + '\n', encoding='utf-8')


def sha256_file(path):
    return hashlib.sha256(pathlib.Path(path).read_bytes()).hexdigest()


def symbol(identifier):
    """The name an id is bound by: its last part (`pkg:explosion/boom` is `boom`)."""
    return re.split(r'[:/]|\$damagetype::', identifier.casefold())[-1]


def clean(text):
    return text.strip().strip('"').strip()


def path_form(text):
    """A file name as a model key spells it: lower case, forward slashes, no `./`."""
    text = text.replace('\\', '/').casefold().strip()
    while text.startswith('./'):
        text = text[2:]
    return text


def strings(value):
    """Every string in a JSON value, including dictionary keys' values only."""
    if isinstance(value, str):
        yield value
    elif isinstance(value, dict):
        for item in value.values():
            yield from strings(item)
    elif isinstance(value, list):
        for item in value:
            yield from strings(item)


class Closure:
    """The records `roots` need, from a weapons pack and its presentation."""

    def __init__(self, weapons, presentation, item_physics=None):
        self.weapons = weapons
        self.presentation = presentation or {}
        self.item_physics = item_physics
        self.records = {}  # (table, key) -> record
        self.full = {}  # casefold full id -> [(table, key)]
        self.bare = {}  # casefold symbol -> [(table, key)]
        self.texture_lookup = {path_form(k): k for k in self.presentation.get('textures') or {}}
        self._index()

    # -- indexing -----------------------------------------------------

    def _add(self, table, key, record, bare):
        node = (table, key)
        self.records[node] = record
        self.full.setdefault(key.casefold(), []).append(node)
        if bare:
            self.bare.setdefault(symbol(key), []).append(node)

    def _index(self):
        w = self.weapons
        for table in ('items', 'images', 'projectiles', 'sounds', 'damage_types', 'explosions'):
            for key, record in (w.get(table) or {}).items():
                self._add(table, key, record, table in SYMBOLIC)
        effects = w.get('effects') or {}
        for kind in ('particles', 'emitters', 'lights', 'explosions'):
            for record in effects.get(kind) or []:
                self._add('fx.' + kind, record['id'], record, True)
        for record in w.get('definitions') or []:
            self._add('definitions', record['name'], record, True)

    def resolve_root(self, name):
        """The item `name` means: its id, its bare name or its on-screen name."""
        wanted = clean(name).casefold()
        found = [n for n in self.full.get(wanted, []) if n[0] == 'items']
        found += [n for n in self.bare.get(wanted, []) if n[0] == 'items'] if not found else []
        if not found:
            for (table, key), record in self.records.items():
                if table != 'items':
                    continue
                if symbol(key) == symbol(wanted) or clean(str(record.get('ui_name', ''))).casefold() == wanted:
                    found.append((table, key))
        return found

    # -- references -----------------------------------------------------

    def _named(self, text):
        """The records the string `text` names."""
        form = clean(text).casefold()
        if not form or len(form) > 300:
            return []
        found = list(self.full.get(form, []))
        if form.startswith('$damagetype::'):
            found += self.bare.get(symbol(form), [])
        elif ':' not in form and '/' not in form:
            found += self.bare.get(form, [])
        return found

    def _models(self):
        """Presentation model key -> record, with the spellings a string may use."""
        return self.presentation.get('models') or {}

    def _model_keys(self, text):
        form = path_form(text)
        if not form.endswith('.dts'):
            return []
        return [key for key in self._models() if key == form or key.endswith('/' + form)]

    def _extras(self, node):
        """Presentation data that goes with a weapons record."""
        table, key = node
        if table in MIRRORED:
            return [(self.presentation.get(table) or {}).get(key)]
        return []

    def references(self, node):
        """Records, model keys and texture keys one record names."""
        record = self.records[node]
        nodes, models, textures = [], [], []
        parts = [record] + [e for e in self._extras(node) if e]
        if node[0] == 'definitions':
            parent = clean(str(record.get('parent') or ''))
            if parent:
                parts.append(parent)
        for part in parts:
            for text in strings(part):
                nodes += self._named(text)
                models += self._model_keys(text)
                found = self.texture_lookup.get(path_form(text))
                if found:
                    textures.append(found)
        return nodes, models, textures

    # -- the closure --------------------------------------------------------

    def compute(self, roots, extra_models=()):
        """Everything `roots` (item names) need. Raises KeyError on an unknown root."""
        pending, needed, models, textures = [], set(), set(), set()
        for name in roots:
            found = self.resolve_root(name)
            if not found:
                raise KeyError(f'No item named {name!r} in this Add-On.')
            pending += found
        for model in extra_models:
            models.update(self._model_keys(model) or ([path_form(model)] if path_form(model) in self._models() else []))
        while pending:
            node = pending.pop()
            if node in needed:
                continue
            needed.add(node)
            more, found_models, found_textures = self.references(node)
            pending += [n for n in more if n not in needed]
            models.update(found_models)
            textures.update(found_textures)
        for key in models:
            textures.update(t for t in self._models()[key].get('textures', []) if t in (self.presentation.get('textures') or {}))
        return Selection(self, needed, models, textures)


class Selection:
    def __init__(self, closure, nodes, models, textures):
        self.closure = closure
        self.nodes = nodes
        self.models = models
        self.textures = textures

    def kept(self, table):
        return {key for (t, key) in self.nodes if t == table}

    def files(self, models=None, textures=None, sounds=None):
        """File names of the kept models, textures and sounds."""
        p = self.closure.presentation
        w = self.closure.weapons
        out = set()
        for key in (self.models if models is None else models):
            out.add(p['models'][key]['file'])
        for key in (self.textures if textures is None else textures):
            out.add(p['textures'][key]['file'])
        for key in (self.kept('sounds') if sounds is None else sounds):
            file = w['sounds'][key].get('file')
            if file:
                out.add(file)
        return out

    def dangling(self):
        """Namespaced ids the kept records name that nothing in the Add-On defines."""
        c = self.closure
        missing = {}
        for node in sorted(self.nodes):
            record = c.records[node]
            for part in [record] + [e for e in c._extras(node) if e]:
                for text in strings(part):
                    form = clean(text)
                    if NAMESPACED.match(form.casefold()) and not c._named(form):
                        missing.setdefault(form, []).append(f'{node[0]}:{node[1]}')
        return {k: sorted(set(v)) for k, v in sorted(missing.items())}

    def report(self):
        c = self.closure
        w = c.weapons
        counts = {}
        for table in ('items', 'images', 'projectiles', 'sounds', 'damage_types', 'explosions', 'definitions'):
            total = len(w.get(table) or [] if table == 'definitions' else w.get(table) or {})
            counts[table] = {'kept': len(self.kept(table)), 'dropped': total - len(self.kept(table))}
        for kind in ('particles', 'emitters', 'lights', 'explosions'):
            total = len((w.get('effects') or {}).get(kind) or [])
            kept = len(self.kept('fx.' + kind))
            counts['effects.' + kind] = {'kept': kept, 'dropped': total - kept}
        p = c.presentation
        counts['models'] = {'kept': len(self.models), 'dropped': len(c._models()) - len(self.models)}
        counts['textures'] = {'kept': len(self.textures),
                              'dropped': len(p.get('textures') or {}) - len(self.textures)}
        return {'items': sorted(self.kept('items')), 'counts': counts, 'dangling': self.dangling()}


def subset(weapons, presentation, item_physics, selection):
    """The weapons pack, presentation and item physics cut to `selection`."""
    c = selection.closure
    w = dict(weapons)
    for table in ('items', 'images', 'projectiles', 'sounds', 'damage_types', 'explosions'):
        if table in w:
            keep = selection.kept(table)
            w[table] = {k: v for k, v in w[table].items() if k in keep}
    if 'definitions' in w:
        keep = {name.casefold() for name in selection.kept('definitions')}
        w['definitions'] = [d for d in w['definitions'] if d['name'].casefold() in keep]
    if 'effects' in w:
        effects = dict(w['effects'])
        for kind in ('particles', 'emitters', 'lights', 'explosions'):
            if kind in effects:
                keep = selection.kept('fx.' + kind)
                effects[kind] = [e for e in effects[kind] if e['id'] in keep]
        w['effects'] = effects
    files = selection.files()
    if 'resources' in w:
        w['resources'] = [r for r in w['resources'] if r.get('native_file') in files]
    p = json.loads(json.dumps(presentation or {}))
    if p:
        p['models'] = {k: v for k, v in (p.get('models') or {}).items() if k in selection.models}
        p['textures'] = {k: v for k, v in (p.get('textures') or {}).items() if k in selection.textures}
        for table in MIRRORED:
            if table in p:
                keep = selection.kept(table)
                p[table] = {k: v for k, v in p[table].items() if k in keep}
    physics = item_physics
    if physics is not None:
        physics = dict(physics)
        keep = selection.kept('items')
        physics['items'] = {k: v for k, v in (physics.get('items') or {}).items() if k in keep}
    return w, p, physics


def prune_package(folder, roots, apply=False, extra_models=()):
    """Cut the Add-On in `folder` to what `roots` need. Returns the report.

    Only files the dropped records owned are deleted, and never one a kept
    record also uses."""
    assets = pathlib.Path(folder) / 'assets'
    weapons = read(assets / 'weapons.json')
    presentation = read(assets / 'presentation.json') if (assets / 'presentation.json').is_file() else {}
    physics_path = assets / 'item-physics.json'
    physics = read(physics_path) if physics_path.is_file() else None
    closure = Closure(weapons, presentation, physics)
    selection = closure.compute(roots, extra_models)
    report = selection.report()
    everything = selection.files(models=set(closure._models()),
                                 textures=set(presentation.get('textures') or {}),
                                 sounds=set(weapons.get('sounds') or {}))
    orphans = sorted(everything - selection.files())
    report['files_removed'] = orphans
    if not apply:
        return report
    w, p, physics = subset(weapons, presentation, physics, selection)
    for name in orphans:
        target = assets / name
        if target.is_file():
            target.unlink()
    write(assets / 'weapons.json', w)
    if physics is not None:
        write(physics_path, physics)
    if p:
        p['weapons_sha256'] = sha256_file(assets / 'weapons.json')
        if physics is not None:
            p['item_physics_sha256'] = sha256_file(physics_path)
        write(assets / 'presentation.json', p)
    return report


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('package', type=pathlib.Path)
    parser.add_argument('items', nargs='+')
    parser.add_argument('--apply', action='store_true')
    parser.add_argument('--json', action='store_true')
    args = parser.parse_args(argv)
    try:
        report = prune_package(args.package, args.items, apply=args.apply)
    except KeyError as error:
        sys.exit(str(error.args[0]))
    if args.json:
        print(json.dumps(report, indent=2))
        return
    verb = 'Kept' if args.apply else 'Would keep'
    for table, count in report['counts'].items():
        print(f'  {table:22} {verb.lower()} {count["kept"]:4}  drop {count["dropped"]:4}')
    for ref, users in report['dangling'].items():
        print(f'  needs {ref} (named by {", ".join(users)}) but the Add-On does not define it')
    if not args.apply:
        print('Nothing changed; add --apply to cut the package down.')


if __name__ == '__main__':
    main()
