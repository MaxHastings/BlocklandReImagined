"""Summarize saved_match_vehicle_routes evidence without assigning bot commands.

Usage: python tools/analyze_bot_match.py <report.json> [--output summary.json]
Reports observations and investigation candidates, not playtest acceptance.
The input and output can contain private saved-build data; keep them local.
"""

import argparse
from collections import Counter, defaultdict
import json
import math
from pathlib import Path


def horizontal(a, b):
    return math.hypot(a[0] - b[0], a[2] - b[2])


def summarize(report):
    vehicles = defaultdict(list)
    for sample in report["samples"]:
        vehicles[sample["id"]].append(sample)
    travel = []
    stationary_vehicles = []
    for vehicle, samples in sorted(vehicles.items()):
        start = samples[0]["position"]
        drivers = []
        still = None
        for s in samples:
            active = [(b, b["driving"]) for b in s["bots"] if b.get("driving")]
            for bot, drive in active:
                drivers.append(bot["id"])
            if still is None or horizontal(s["position"], still["position"]) > 3:
                if still and still["attempted_seconds"] >= 10:
                    stationary_vehicles.append(still)
                still = {"vehicle": vehicle, "first_second": s["second"], "last_second": s["second"],
                         "position": s["position"], "attempted_seconds": 0, "drivers": []}
            still["last_second"] = s["second"]
            for bot, drive in active:
                if bot.get("goal") and horizontal(s["position"], bot["goal"]) > 10:
                    still["attempted_seconds"] += 1
                    if bot["id"] not in still["drivers"]:
                        still["drivers"].append(bot["id"])
        if still and still["attempted_seconds"] >= 10:
            stationary_vehicles.append(still)
        travel.append({
            "vehicle": vehicle,
            "definition": samples[0]["definition"],
            "first_second": samples[0]["second"],
            "last_second": samples[-1]["second"],
            "spawn": start,
            "max_displacement": round(max(horizontal(s["position"], start) for s in samples), 2),
            "observed_drivers": sorted(set(drivers)),
            "driver_changes": sum(a != b for a, b in zip(drivers, drivers[1:])),
            "x_range": [min(s["position"][0] for s in samples), max(s["position"][0] for s in samples)],
            "z_range": [min(s["position"][2] for s in samples), max(s["position"][2] for s in samples)],
        })
    bots = defaultdict(list)
    for sample in report.get("bot_samples", []):
        bots[sample["id"]].append(sample)
    behaviour = Counter()
    diagnoses = Counter()
    gates = Counter()
    driving = Counter()
    selected_weapons = Counter()
    goofs = Counter()
    choices = Counter()
    seen_choices = set()
    stationary = []
    lives = []

    def retain(run):
        if run and run["last_second"] - run["first_second"] >= 10:
            stationary.append(run)

    for bot, samples in sorted(bots.items()):
        spawn_ticks = set()
        fired_ticks = set()
        deaths = set()
        run = None
        for s in samples:
            vital = s.get("vitals") or {}
            state = s.get("state") or {}
            alive = vital.get("alive", False) and not vital.get("held", False)
            if "spawn_tick" in vital:
                spawn_ticks.add(vital["spawn_tick"])
            if vital.get("fired_tick") is not None:
                fired_ticks.add(vital["fired_tick"])
            if vital.get("died_tick") is not None:
                deaths.add(vital["died_tick"])
            if not alive:
                retain(run)
                run = None
                continue
            behaviour[s["behaviour"]] += 1
            inventory = s.get("inventory") or {}
            slot = inventory.get("selected")
            if slot is not None and slot < len(inventory.get("slots", [])):
                selected_weapons[inventory["slots"][slot]] += 1
            if s.get("goof"):
                goofs[s["goof"]] += 1
            for d in s.get("decisions", []):
                key = (bot, d["domain"], d["tick"])
                if key not in seen_choices:
                    seen_choices.add(key)
                    choices[(d["domain"], d["chosen"], d["reason"])] += 1
            if s.get("objective_diagnostic"):
                diagnoses[s["objective_diagnostic"]] += 1
            if s.get("gate"):
                gates[s["gate"][1]] += 1
            drive = s.get("driving")
            if drive:
                driving["samples"] += 1
                for reason in ("obstacle", "edge", "waiting", "recovering", "brake"):
                    driving[reason] += bool(drive[reason])
                driving["idle_at_least_6_seconds"] += drive["idle_ticks"] >= 720
            intent = s.get("walking_intent") or {}
            point = state.get("feet")
            walking = not vital.get("mounted") and max(abs(intent.get("forward", 0)), abs(intent.get("right", 0))) > 0.2
            # Deliberate combat strafing and observation pauses are not stuck travel.
            travelling = s["behaviour"] in ("objective", "chase", "search", "interact", "explore", "return")
            no_route = not s.get("next") and not s.get("searching") and travelling
            # A visible enemy does not explain stalled objective travel: a
            # committed delivery still walks its route while firing back.
            # Retain that evidence; deliberate fighting/strafe is excluded by
            # the selected behaviour rather than by visibility alone.
            candidate = not vital.get("mounted") and travelling and (walking or no_route) and s.get("goal") is not None and point is not None
            candidate = candidate and horizontal(point, s["goal"]) > 3
            detail = s.get("objective_detail")
            candidate = candidate and not s.get("goof") and not (isinstance(detail, dict) and detail.get("phase") == "waiting")
            if candidate:
                same = run and run["spawn_tick"] == vital.get("spawn_tick") and s["second"] == run["last_second"] + 1
                if not same or horizontal(point, run["position"]) > 2:
                    retain(run)
                    run = {
                        "bot": bot, "spawn_tick": vital.get("spawn_tick"),
                        "first_second": s["second"], "last_second": s["second"],
                        "position": point, "behaviour": s["behaviour"], "goal": s["goal"],
                        "short": s.get("short"), "objective_diagnostic": s.get("objective_diagnostic"),
                        "visible": s.get("visible"), "acted": s.get("acted"),
                        "intent": "walking" if walking else "no route",
                    }
                else:
                    run["last_second"] = s["second"]
            else:
                retain(run)
                run = None
        retain(run)
        lives.append({
            "bot": bot, "observed_bodies": len(spawn_ticks), "observed_deaths": len(deaths),
            "observed_shot_ticks": len(fired_ticks),
            "final_score": samples[-1].get("vitals", {}).get("score"),
        })
    stationary.sort(key=lambda s: s["last_second"] - s["first_second"], reverse=True)
    # Infer the field axis from actual initial team spawns. No map names,
    # authored waypoints or required bot destinations enter this measurement.
    spawns = defaultdict(list)
    for samples in bots.values():
        first = samples[0]
        if first.get("state") and first.get("vitals", {}).get("team") is not None:
            spawns[first["vitals"]["team"]].append(first["state"]["feet"])
    crossings = []
    if len(spawns) == 2:
        centres = {t: [sum(p[i] for p in points) / len(points) for i in range(3)] for t, points in spawns.items()}
        for samples in bots.values():
            prior = None
            recorded = set()
            for s in samples:
                vital, state = s.get("vitals") or {}, s.get("state") or {}
                team, point = vital.get("team"), state.get("feet")
                if team not in centres or not point or not vital.get("alive"):
                    prior = None
                    continue
                origin = centres[team]
                other = next(p for t, p in centres.items() if t != team)
                size = horizontal(origin, other)
                if size < 1:
                    continue
                dx, dz = (other[0] - origin[0]) / size, (other[2] - origin[2]) / size
                progress = ((point[0] - origin[0]) * dx + (point[2] - origin[2]) * dz) / size
                life = vital["spawn_tick"]
                if prior and prior[0] == life:
                    for fraction in (0.25, 0.5, 0.75):
                        key = (life, fraction)
                        if key not in recorded and prior[1] < fraction <= progress:
                            recorded.add(key)
                            crossings.append({"bot": s["id"], "team": team, "spawn_tick": life,
                                              "second": s["second"], "fraction": fraction, "position": point,
                                              "mounted": bool(vital.get("mounted")),
                                              "lateral_offset": round(-(point[0] - origin[0]) * dz + (point[2] - origin[2]) * dx, 2)})
                prior = (life, progress)
    damage = defaultdict(lambda: {"hits": 0, "health_lost": 0.0, "victims": set()})
    for hit in report.get("damage", []):
        key = hit.get("projectile") or "non-projectile damage"
        damage[key]["hits"] += 1
        damage[key]["health_lost"] += hit["amount"]
        damage[key]["victims"].add(hit["victim"])
    damage_summary = {key: {"hits": value["hits"], "health_lost": round(value["health_lost"], 2),
                            "victims": sorted(value["victims"])} for key, value in damage.items()}
    frames = defaultdict(list)
    for samples in bots.values():
        for sample in samples:
            frames[sample["second"]].append(sample)
    spacing = Counter()
    following = 0
    team_terms = Counter()
    for frame in frames.values():
        live = [s for s in frame if s.get("state") and s.get("vitals", {}).get("alive")
                and not s["vitals"].get("mounted")]
        for s in live:
            team = s["vitals"].get("team")
            if team is None:
                continue
            position = s["state"]["feet"]
            if len(spawns) == 2 and team in centres:
                origin = centres[team]
                other = next(p for t, p in centres.items() if t != team)
                size = horizontal(origin, other)
                progress = sum((position[i] - origin[i]) * (other[i] - origin[i]) for i in (0, 2)) / max(size * size, 1)
                if not 0.15 < progress < 0.85:
                    continue
            allies = [a for a in live if a["id"] != s["id"] and a["vitals"].get("team") == team]
            if not allies:
                spacing["no_living_foot_ally"] += 1
                continue
            ally = min(allies, key=lambda a: horizontal(position, a["state"]["feet"]))
            distance = horizontal(position, ally["state"]["feet"])
            spacing["under_2_units" if distance < 2 else "2_to_6_units" if distance < 6
                    else "6_to_15_units" if distance < 15 else "over_15_units"] += 1
            a, b = s["state"]["velocity"], ally["state"]["velocity"]
            speed_a, speed_b = math.hypot(a[0], a[2]), math.hypot(b[0], b[2])
            if 2 < distance < 12 and min(speed_a, speed_b) > 1:
                aligned = (a[0] * b[0] + a[2] * b[2]) / (speed_a * speed_b) > 0.7
                delta = [ally["state"]["feet"][i] - position[i] for i in (0, 2)]
                ahead = (delta[0] * a[0] + delta[1] * a[2]) / speed_a
                lateral = abs(delta[0] * a[2] - delta[1] * a[0]) / speed_a
                following += aligned and ahead > 0 and lateral < 2
            for name, overlap, uses in (s.get("team") or {}).get("terms", []):
                if overlap:
                    team_terms[name + ": overlap"] += 1
                if uses:
                    team_terms[name + ": support"] += 1
    return {
        "world": report.get("world"), "seconds": report.get("seconds"),
        "bot_step_failures": report.get("bot_step_failures"),
        "vehicle_travel": travel, "alive_bot_behaviour_samples": dict(behaviour.most_common()),
        "stationary_vehicle_candidates": sorted(stationary_vehicles, key=lambda s: s["attempted_seconds"], reverse=True),
        "objective_diagnostic_samples": dict(diagnoses.most_common()),
        "fire_gate_samples": dict(gates.most_common()), "driving_samples": dict(driving),
        "bot_lifecycles": lives, "stationary_travel_candidates": stationary[:20],
        "field_crossings": crossings,
        "selected_weapon_samples": dict(selected_weapons.most_common()),
        "explicit_goof_samples": dict(goofs.most_common()),
        "decisions": [{"domain": key[0], "chosen": key[1], "reason": key[2], "count": count}
                      for key, count in choices.most_common()],
        "observed_projectile_types": dict(Counter(p["definition"] for p in report.get("projectiles", [])).most_common()),
        "actual_damage": damage_summary, "actual_round_outcomes": report.get("rounds", []),
        "foot_ally_spacing_in_field_samples": dict(spacing),
        "following_like_motion_samples": following,
        "team_coordination_term_samples": dict(team_terms),
        "limits": "One-second observations can miss brief events. Stationary travel candidates require investigation; they are not automatically defects.",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    result = summarize(json.loads(args.report.read_text(encoding="utf-8")))
    text = json.dumps(result, indent=2) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(text, encoding="utf-8")
    else:
        print(text, end="")


if __name__ == "__main__":
    main()
