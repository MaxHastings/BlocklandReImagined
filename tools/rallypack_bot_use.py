"""Describe the constant-velocity package shot to v0.2.8's bot executor.

The image command still owns firing. Keeping its actual projectile reference
lets the existing scripted-weapon executor read flight, range and blast.
Variable-charge or cooked attacks need a richer provider and remain unsupported.

`ammo` says where the package's rules keep the weapon's rounds, so a bot sees
an empty gun as empty and a gun with spare magazines as still in play. It
reads the count (Image::bot.ammo) and never changes it.
"""


def state_ref(key, field=""):
    ref = {"key": key}
    if field:
        ref["field"] = field
    return ref


def ammo_in_bank(package, bank, magazines):
    """Rounds kept as one object per ammo type: `n` loaded, and either `mags`
    (spare magazines, for a magazine gun) or `reserve` (loose rounds) to
    reload from. Only the one the gun reloads from counts as spare."""
    return {"package": package, "loaded": state_ref(bank, "n"),
            "spare": [state_ref(bank, "mags" if magazines else "reserve")]}


def describe(image, projectile, ammo=None):
    image["projectile"] = projectile
    # Follow only transitions available with trigger held and ammunition present.
    # A second visit to a firing state proves the authored cycle is automatic;
    # semi-automatic cycles stop until trigger-up instead.
    states = image["states"]
    fire = next((i for i, s in enumerate(states)
                 if s.get("script", "").lower() == "onfire"), None)
    automatic = False
    if fire is not None:
        pending = [fire]
        visited = set()
        while pending:
            at = pending.pop()
            if at in visited:
                continue
            visited.add(at)
            state = states[at]
            for edge in ("timeout", "down", "ammo", "loaded"):
                target = state.get(edge)
                if target is None:
                    continue
                if target == fire:
                    automatic = True
                elif target not in visited:
                    pending.append(target)
    image["bot"] = {"fire": "hold" if automatic else "tap"}
    if ammo:
        image["bot"]["ammo"] = ammo
