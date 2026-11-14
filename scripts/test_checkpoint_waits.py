"""Deterministic stream-boundary regression for the checkpoint workload."""
import copy
import unittest
from test_checkpoint_process import CheckpointFixture


class CheckpointWaitCompletion(CheckpointFixture, unittest.TestCase):
    def test_started_frame_and_stale_permissions_do_not_qualify(self):
        started = {"state": {"observation": {"tick": "13160", "ready": False,
                   "combat": {"dead": False, "preparation_active": True}}},
                   "intentions": [{"phase": "started"}],
                   "readiness": {"admission": True, "resume": [], "cancel": ["wait"]}}
        observed = copy.deepcopy(started)
        observed["state"]["observation"].update(tick="13200", ready=True,
            combat={"dead": False, "preparation_active": False})
        resolved = {**observed, "intentions": []}
        settled = {**resolved, "readiness": {"admission": True, "resume": [], "cancel": []}}
        def frame(player, predicate):
            self.assertEqual(player, "player")
            for intermediate in (started, observed, resolved):
                self.assertFalse(predicate(intermediate))
            self.assertTrue(predicate(settled))
            return settled
        self.frame = frame
        self.assertIs(self.settled_waits("player", started, 132), settled)

    def test_already_completed_picture_does_not_wait_for_an_extra_frame(self):
        settled = {"state": {"observation": {"tick": "13200", "ready": True,
                   "combat": {"dead": False, "preparation_active": False}}},
                   "intentions": [], "readiness": {"admission": True, "resume": [], "cancel": []}}
        self.frame = lambda *_: self.fail("Already completed picture must be retained")
        self.assertIs(self.settled_waits("player", settled, 132), settled)
