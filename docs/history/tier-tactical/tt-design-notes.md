> **Historical note. Written 2026-10-01 (the file's date in the cloud project folder, `tier-tactical/tt-design-notes.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Tier+Tactical Kai: design notes (our own summary, no original code)

Source: forum topic 324336 "Tier+Tactical Kai & Extras", read on Max's PC
(full notes: C:\Users\Maxwell\Desktop\Games\.research\tier-tactical\tt-mechanics.md).

## Credits
Bushido (original T+T creator, Sawn-Off, Frog's co-author), Space Guy (co-author
of every pack, raycast support with Iban, Impact Rifle co-author, SpecialKills),
Jaydee (Melee 1, Explosive 1-2, Tier 2, many skins), Tingalz (Melee 2, Medical,
Bullpup/Dualies/LMG/MP skins), Panopticon (Kai revamp: ammo rewrite, prefs,
AddAmmo event; forum account TheViewer), Frog (Frog's Weaponry, WWII),
Heedicalking (Impact Rifle).

## Structure
Tier1 = core (ammo system, raycasting, alt trigger, alt datablocks, prefs) + 5 guns.
Tier1A, Tier2, Tier2A, Explosive1/2, extras require Tier1. Medic needs only
Weapon_Gun; Melee stands alone (Melee II needs Melee I). Skins are separate
items copying a base gun's stats. 23 add-ons in the Kai bundle + 5 extras.

## Mechanics that matter for seams
- Ammo: reserve per player per ammo type (9mm, 5.56, shotgun, .270, .880,
  7.08, bomb, rocket, bolt...), magazine per inventory slot. Four modes:
  mag+reserve (default), mag only infinite reloads, classic infinite, arena
  (every shot from reserve). Single-shot launchers always reload.
- Reload: auto when empty, Light key manual; full vs partial path; rounds
  move at the end; switching cancels; shell-by-shell loops for shotguns and
  revolvers, interruptible by trigger.
- HUD: bottomPrint "ammo name" + "mag / reserve", 4 s, refreshed on events.
- Death drops an ammo bag (12 s life) with the reserve; ammo pickups by touch.
- Shooting: hitscan (pistols, MPs, snipers, carbine, melee, laser) vs
  projectile (the rest). No falloff, no penetration. Pellet shotguns + a short
  "blast" projectile.
- Spread: fixed per gun, chosen by moving (speed > 0.1 or 4) and "rested"
  (time since last shot > 200/500/1000 ms). AR: first rested shot stronger.
- Recoil: camera shake (3 levels: freq 1/2/3, amp .1-.3/.3-.5/.6-.8, 0.5 s),
  self-kick on a few guns, arm animation per shot.
- Hit slowdown: target velocity divided (1.1/1.2/1.5) and max speed scaled
  down to 1/(3*divisor), reset 200 ms after last hit.
- Movement: LMG firing / carbine zoom -> slow body (speed 4, no jet);
  Vulcan spin-up / Payload -> slowed; deployed Vulcan -> immobile.
- Dual wield: right gun mounts left in image slot 1, pulses its trigger
  after each right shot; shared magazine.
- Fire modes fixed per gun (semi/auto/3-burst). Alt (jet key) on a few guns.
- Explosives: conc (auto-throw, 3rd bounce explodes), stick (hold/release,
  4 s fuse), molotov (impact, embers that bounce and burn radius 4 for 4 dmg
  every 300 ms for 15 s), RPG, GL, Flak (sprays sparks every 200 ms and on
  bounce), no C4/mines.
- Medic: gauze dart gun (10 + 2/tick heal over time up to 40), syringe
  (self 20 with 5 s cooldown, dart 30). Healing respects minigame/teams.
- Melee: eye ray 3-7 units, 35-105 dmg, swing timings; knife charge;
  chainsaw 15/0.05 s; riot shield blocks by facing (5% dmg, reflects
  projectiles, 20 blocks durability).
- Headshot test in the original is probably broken (threshold below the box).
