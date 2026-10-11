"""What the Rallypack recipes tell bots about a rule-fired gun; no content needed."""
import unittest

import rallypack_bot_use as bot_use


def image(automatic):
    """Automatic: Fire times out into Ready, which fires again while the trigger is held.
    Semi-automatic: Fire times out into a state that waits for the trigger to be released."""
    states = [
        {"name": "Ready", "script": "", "down": 1},
        {"name": "Fire", "script": "onFire", "timeout": 0 if automatic else 2},
        {"name": "Release", "script": "", "up": 0},
    ]
    return {"states": states}


class Describe(unittest.TestCase):
    def test_a_cycle_that_returns_to_fire_is_held_and_one_that_returns_to_ready_is_tapped(self):
        held, tapped = image(True), image(False)
        bot_use.describe(held, "p:projectile/x")
        bot_use.describe(tapped, "p:projectile/x")
        self.assertEqual(held["bot"], {"fire": "hold"})
        self.assertEqual(tapped["bot"], {"fire": "tap"})
        self.assertEqual(held["projectile"], "p:projectile/x")

    def test_ammo_says_where_the_rules_keep_the_rounds(self):
        one = image(True)
        bot_use.describe(one, "p:projectile/x", bot_use.ammo_in_bank("rules", "ammo_9mm", True))
        self.assertEqual(one["bot"]["ammo"], {
            "package": "rules",
            "loaded": {"key": "ammo_9mm", "field": "n"},
            "spare": [{"key": "ammo_9mm", "field": "mags"}],
        })

    def test_only_the_pool_a_gun_reloads_from_counts_as_spare(self):
        loose = bot_use.ammo_in_bank("rules", "ammo_rocket", False)
        self.assertEqual(loose["spare"], [{"key": "ammo_rocket", "field": "reserve"}])

    def test_the_p90_keeps_rounds_and_spares_under_separate_keys(self):
        gun = image(True)
        ammo = {"package": "rules", "loaded": bot_use.state_ref("rounds"),
                "spare": [bot_use.state_ref("spares")]}
        bot_use.describe(gun, "p:projectile/x", ammo)
        self.assertEqual(gun["bot"]["ammo"]["loaded"], {"key": "rounds"})

    def test_no_ammo_leaves_the_description_as_it_was(self):
        gun = image(True)
        bot_use.describe(gun, "p:projectile/x")
        self.assertNotIn("ammo", gun["bot"])


if __name__ == "__main__":
    unittest.main()
