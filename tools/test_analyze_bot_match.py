import unittest

from analyze_bot_match import summarize


class TravelCandidates(unittest.TestCase):
    def report(self, behaviour):
        return {"samples": [], "bot_samples": [
            {"id": 1, "second": second, "behaviour": behaviour,
             "vitals": {"alive": True, "spawn_tick": 1},
             "state": {"feet": [0, 0, 0]}, "goal": [20, 0, 0],
             "walking_intent": {"forward": 1}, "visible": 2}
            for second in range(12)
        ]}

    def test_visible_enemy_does_not_hide_a_stalled_objective(self):
        candidates = summarize(self.report("objective"))["stationary_travel_candidates"]
        self.assertEqual(len(candidates), 1)
        self.assertEqual(candidates[0]["visible"], 2)

    def test_fighting_footwork_is_not_classified_as_stalled_travel(self):
        self.assertEqual(summarize(self.report("fight"))["stationary_travel_candidates"], [])


if __name__ == "__main__":
    unittest.main()
