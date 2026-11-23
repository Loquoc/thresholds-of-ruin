"""Real server/client acceptance for the complete peaceful mob hall."""
import unittest

from process_harness import ProcessTestCase


class PeacefulMenagerieProcesses(ProcessTestCase):
    def test_waiting_among_the_catalog_mobs_keeps_the_visitor_unharmed(self):
        self.server(scenario="peaceful-menagerie")
        client, initial = self.client()
        before = initial["state"]["observation"]
        hp = before["combat"]["hp"]
        start = int(before["tick"])
        for _ in range(30):
            frame = self.act(client, {"type": "wait"})
            self.assertIsNone(frame.get("error"))
            observation = frame["state"]["observation"]
            self.assertEqual(observation["combat"]["hp"], hp)
            self.assertFalse(observation["combat"]["dead"])
        self.assertGreater(int(observation["tick"]), start)


if __name__ == "__main__":
    unittest.main()
