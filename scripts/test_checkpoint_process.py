"""Actual clients exercise checkpoint rotation, crash rollback and restart."""
import copy
import os
import sqlite3
import subprocess
import json
import unittest
import zlib
import re
import tomllib

from checkpoint_payload import HEADER, crc32c, decode_checkpoint_payload
from process_harness import ROOT, ProcessTestCase, SPECTATOR_TOKEN, TOKEN, creature_package


SAVE_VERSION = int(re.search(r"pub\(crate\) const ARCHIVE_VERSION: u32 = (\d+);",
    (ROOT / "crates/server/src/engine.rs").read_text(encoding="utf-8")).group(1))


class CheckpointFixture:
    def saving_server(self, target=3600000, maximum=7200000, *, writer_barrier=False):
        """A server that checkpoints every four actions, with explicit save timing."""
        # Eight journal records cover four completed gameplay intentions. Keep a
        # nonempty execution tail so this exercises checkpoint-plus-tail replay.
        return self.server("--checkpoint-interval", 8, "--save-target-ms", target, "--save-max-ms", maximum,
                           "--save-idle-ms", 0, seed=None, writer_barrier=writer_barrier)



class CheckpointProcesses(CheckpointFixture, ProcessTestCase):
    def read_checkpoint(self):
        with sqlite3.connect(self.save) as db:
            self.assertEqual(db.execute("PRAGMA user_version").fetchone()[0], SAVE_VERSION)
            payload, checksum = db.execute("SELECT payload, checksum FROM checkpoint").fetchone()
        self.assertEqual(crc32c(b"123456789"), 0xe3069283)
        self.assertEqual(crc32c(payload), checksum)
        checkpoint = json.loads(decode_checkpoint_payload(payload))
        self.assertEqual(checkpoint["version"], SAVE_VERSION)
        return checkpoint

    def write_checkpoint(self, checkpoint):
        raw = json.dumps(checkpoint, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
        payload = HEADER + zlib.compress(raw, 1) + len(raw).to_bytes(4, "little")
        self.assertEqual(decode_checkpoint_payload(payload), raw)
        checksum = crc32c(payload)
        with sqlite3.connect(self.save) as db:
            updated = db.execute("UPDATE checkpoint SET payload = ?, checksum = ?",
                                 (payload, checksum))
            self.assertEqual(updated.rowcount, 1)
            self.assertEqual(db.execute("SELECT payload, checksum FROM checkpoint").fetchone(),
                             (payload, checksum))

    def assert_invalid_save(self, *, scenario=None):
        before = self.save.read_bytes()
        result = subprocess.run(
            [self.bin / ("tor-server" + self.suffix), "--listen", "127.0.0.1:0",
             *(["--scenario", scenario] if scenario is not None else []), "--save", self.save],
            capture_output=True, text=True, encoding="utf-8", timeout=15,
            env={**os.environ, "TOR_SERVER_TOKEN": TOKEN})
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn('"address"', result.stdout, "an invalid save must never advertise a listener")
        self.assertIn("InvalidArchive", result.stderr)
        self.assertIn("unsupported or inconsistent game journal", result.stderr.lower())
        self.assertEqual(self.save.read_bytes(), before)

    def test_playable_rogue_rejects_previous_save_archive_and_checkpoint_before_rewrite(self):
        server = self.server("--checkpoint-interval", 1)
        player, _ = self.client()
        self.assertIsNone(self.act(player, {"type": "wait"})["error"])
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        player.stop()
        server.stop()
        checkpoint = self.read_checkpoint()
        original = self.save.read_bytes()
        # Adjacent unsupported identities at every independent persistence boundary.
        for obsolete in (SAVE_VERSION - 1, SAVE_VERSION + 1):
            for boundary in ("sqlite", "archive", "checkpoint"):
                with self.subTest(boundary=boundary, obsolete=obsolete):
                    self.save.write_bytes(original)
                    if boundary == "sqlite":
                        with sqlite3.connect(self.save) as db:
                            db.execute(f"PRAGMA user_version = {obsolete}")
                    elif boundary == "archive":
                        with sqlite3.connect(self.save) as db:
                            frame = db.execute("SELECT frame FROM journal WHERE sequence=0").fetchone()[0]
                            self.assertEqual(frame[:4], b"TORB")
                            self.assertEqual(int.from_bytes(frame[4:6], "little"), 6)
                            self.assertEqual(int.from_bytes(frame[6:16], "little"), 0)
                            self.assertEqual(int.from_bytes(frame[16:20], "little"), len(frame) - 24)
                            self.assertEqual(int.from_bytes(frame[20:24], "little"),
                                             crc32c(frame[4:20] + frame[24:]))
                            base = json.loads(frame[24:])
                            self.assertEqual(base["archive"]["version"], SAVE_VERSION)
                            base["archive"]["version"] = obsolete
                            raw = json.dumps(base, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
                            header = frame[:16] + len(raw).to_bytes(4, "little")
                            changed = header + crc32c(header[4:] + raw).to_bytes(4, "little") + raw
                            self.assertNotEqual(changed, frame)
                            db.execute("UPDATE journal SET frame = ? WHERE sequence=0", (changed,))
                    else:
                        changed = copy.deepcopy(checkpoint)
                        changed["version"] = obsolete
                        self.write_checkpoint(changed)
                    self.assert_invalid_save()

    def test_checkpoint_requires_health_recovery_and_build_fields_before_listening(self):
        package = creature_package(self, "required-health-ledger-checkpoint", abilities=("fixture_focus_fear",))
        server = self.server("--checkpoint-interval", 1, scenario=package)
        player, _ = self.client()
        saved = self.act(player, {"type": "wait"})
        self.assertIsNone(saved["error"])
        saved = self.frame(player, lambda frame: frame.get("state") is not None
                           and frame["state"]["observation"]["ready"])
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        player.stop()
        server.stop()
        checkpoint = self.read_checkpoint()
        # Positive control: the same test encoder and CRC must produce a real,
        # restartable save before removing logical fields one at a time.
        self.write_checkpoint(checkpoint)
        server = self.server(scenario=package, seed=None)
        player, resumed = self.client()
        self.assertDisclosedStateEqual(resumed["state"], saved["state"])
        player.stop()
        server.stop()
        checkpoint = self.read_checkpoint()
        original = self.save.read_bytes()
        for path in (
            ("health_recovery",),
            ("nutrition",),
            ("health_recovery", "elapsed"),
            ("health_recovery", "nutrition"),
            ("build", "starting_skills"),
            ("build", "restoration"),
            ("build", "choices", 0, "purchases"),
        ):
            with self.subTest(field=path):
                self.save.write_bytes(original)
                broken = copy.deepcopy(checkpoint)
                changed = 0
                for actor in broken["shared"]["actor_records"]:
                    if actor.get("combat") is None:
                        continue
                    parent = actor["combat"]["creature"]
                    for part in path[:-1]:
                        parent = parent[part]
                    self.assertIn(path[-1], parent)
                    del parent[path[-1]]
                    changed += 1
                self.assertGreater(changed, 0)
                self.write_checkpoint(broken)
                self.assert_invalid_save(scenario=package)

    def test_d20_save_format_is_rejected_before_replay_or_rewrite(self):
        import subprocess
        server = self.saving_server()
        player, _ = self.client()
        self.assertIsNone(self.act(player, {"type": "wait"})["error"])
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        player.stop()
        server.stop()
        with sqlite3.connect(self.save) as db:
            self.assertEqual(db.execute("PRAGMA user_version").fetchone()[0], SAVE_VERSION)
            # The legacy d20 format owns a different deterministic check
            # stream. Even otherwise valid payloads must not cross this gate.
            db.execute("PRAGMA user_version = 27")
        before = self.save.read_bytes()
        result = subprocess.run(
            [self.bin / ("tor-server" + self.suffix), "--listen", "127.0.0.1:0",
             "--save", self.save], capture_output=True, text=True, encoding="utf-8",
            timeout=15, env={**os.environ, "TOR_SERVER_TOKEN": TOKEN})
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn('"address"', result.stdout)
        self.assertIn("InvalidArchive", result.stderr)
        self.assertEqual(self.save.read_bytes(), before)

    def test_restart_rejects_legacy_raw_combat_without_rewriting_the_save(self):
        package = creature_package(self, "owned-checkpoint", abilities=("fixture_focus_fear",))
        server = self.server("--checkpoint-interval", 1, scenario=package)
        player, _ = self.client()
        self.assertIsNone(self.act(player, {"type": "wait"})["error"])
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        player.stop()
        server.stop()
        legacy = {"spec": {"name": "figure", "max_hp": 30, "defense": 10,
                           "attack": {"bonus": 2, "wind_up": 60, "recovery": 40, "damage": {"impact": 4}},
                           "immunities": [], "reductions": {}, "faction": "neutral"}, "hp": 30}
        checkpoint = self.read_checkpoint()
        changed = 0
        for actor in checkpoint["shared"]["actor_records"]:
            if actor.get("combat") is not None:
                self.assertEqual(set(actor["combat"]), {"identity", "creature"})
                actor["combat"] = legacy
                changed += 1
        self.assertGreater(changed, 0)
        self.write_checkpoint(checkpoint)
        self.assert_invalid_save(scenario=package)


    def test_checkpoint_tail_crash_and_restart_through_real_headless_client(self):
        server = self.saving_server()
        player, _ = self.client()
        for _ in range(10):
            saved = self.act(player, {"type": "wait"})
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        with sqlite3.connect(self.save) as db:
            self.assertEqual(db.execute("SELECT sequence FROM checkpoint").fetchone()[0], 16)
            payload = db.execute("SELECT payload FROM checkpoint").fetchone()[0]
            raw = decode_checkpoint_payload(payload)
            checkpoint = json.loads(raw)
            self.assertEqual(checkpoint["version"], SAVE_VERSION)
            self.assertEqual(checkpoint["sequence"], 16)
            self.assertLess(len(payload), len(raw))
            self.assertEqual(db.execute("SELECT count(*) FROM history").fetchone()[0], 16)
            self.assertEqual(db.execute("SELECT count(*) FROM journal WHERE sequence>0").fetchone()[0], 4)
        for _ in range(3):
            self.act(player, {"type": "wait"})
        server.child.kill(); server.child.wait(timeout=10)
        self.saving_server()
        player, resumed = self.client()
        for frame in (saved, resumed):
            # The checkpoint-tail fixture has an ordinary body, not a creature.
            self.assertNotIn("combat", frame["state"]["observation"])
        self.assertDisclosedStateEqual(resumed["state"], saved["state"])
        self.assertEqual(resumed["history"], saved["history"])
        for _ in range(3):
            continued = self.act(player, {"type": "wait"})
        self.assertEqual(continued["state"]["observation"]["tick"], "1300")
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        with sqlite3.connect(self.save) as db:
            self.assertEqual(db.execute("SELECT sequence FROM checkpoint").fetchone()[0], 24)


    def test_checkpoint_writer_wait_does_not_block_new_actions(self):
        self.saving_server(target=1, maximum=1000, writer_barrier=True)
        player, _ = self.client()
        # Reach the checkpoint boundary using a durable prefix, then hold the
        # next batch while more accepted actions execute.
        for _ in range(3):
            self.act(player, {"type": "wait"})
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        with self.held_save_writer() as barrier:
            self.act(player, {"type": "wait"})
            barrier.wait()
            for _ in range(2):
                result = self.act(player, {"type": "wait"})
            self.assertEqual(result["state"]["observation"]["tick"], "600")
            barrier.assert_held()
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        barrier.assert_released()
        with sqlite3.connect(self.save) as db:
            self.assertEqual(db.execute("SELECT sequence FROM checkpoint").fetchone()[0], 8)


    def capacity_server(self, *, reduced=False, scenario="two-room"):
        return self.server("--checkpoint-interval", 8, "--save-target-ms", 3600000,
            "--save-max-ms", 7200000, "--save-idle-ms", 0, scenario=scenario, seed=None,
            extra_env={"TOR_TEST_CHECKPOINT_BYTE_LIMIT": "0"} if reduced else {})

    def test_capacity_deferral_keeps_saved_tail_with_and_without_old_checkpoint(self):
        # Real encoder bounds, not an alternate serializer or a fabricated error.
        # The ordinary journal records remain legal at the production limit.
        for prior_checkpoint in (False, True):
            with self.subTest(prior_checkpoint=prior_checkpoint):
                self.save = self.directory / f"capacity-{prior_checkpoint}.db"
                previous = None
                if prior_checkpoint:
                    server = self.capacity_server()
                    player, _ = self.client()
                    for _ in range(4):
                        self.assertIsNone(self.act(player, {"type": "wait"})["error"])
                    self.assertIsNone(self.request(player, {"type": "save"})["error"])
                    player.stop(); server.stop()
                    with sqlite3.connect(self.save) as db:
                        previous = db.execute("SELECT sequence,payload,checksum FROM checkpoint").fetchone()
                    self.assertIsNotNone(previous)
                server = self.capacity_server(reduced=True)
                player, _ = self.client()
                for _ in range(5):
                    completed = self.act(player, {"type": "wait"})
                    self.assertIsNone(completed["error"])
                for _ in range(2):
                    self.assertIsNone(self.request(player, {"type": "save"})["error"])
                # Further ordinary play remains admitted after the first overflow.
                completed = self.act(player, {"type": "wait"})
                self.assertIsNone(completed["error"])
                self.assertIsNone(self.request(player, {"type": "save"})["error"])
                warnings = [json.loads(line) for line in player.transcript if line.startswith("{")]
                warnings = [frame.get("message") or {} for frame in warnings]
                self.assertTrue(any(message.get("type") == "error"
                    and message.get("request_id") is None
                    and "Checkpoint deferred" in message.get("message", "")
                    and "journal saving continues" in message.get("message", "")
                    for message in warnings))
                with sqlite3.connect(self.save) as db:
                    self.assertEqual(db.execute("SELECT sequence,payload,checksum FROM checkpoint").fetchone(), previous)
                    tail = db.execute("SELECT sequence,frame FROM journal WHERE sequence>0 ORDER BY sequence").fetchall()
                    self.assertTrue(tail)
                    self.assertGreater(tail[0][0], previous[0] if previous else 0)
                    self.assertEqual([seq for seq, _ in tail], list(range(tail[0][0], tail[-1][0] + 1)))
                player.stop()
                server.child.kill(); server.child.wait(timeout=10)
                # A fresh normal process replays the old checkpoint plus durable
                # tail, or the base plus tail when no checkpoint ever succeeded.
                server = self.capacity_server()
                player, restored = self.client()
                self.assertDisclosedStateEqual(restored["state"], completed["state"])
                self.assertEqual(restored["history"], completed["history"])
                for _ in range(4):
                    self.assertIsNone(self.act(player, {"type": "wait"})["error"])
                self.assertIsNone(self.request(player, {"type": "save"})["error"])
                with sqlite3.connect(self.save) as db:
                    selected = db.execute("SELECT sequence FROM checkpoint").fetchone()[0]
                self.assertGreater(selected, tail[-1][0])
                player.stop(); server.stop()

    def test_invalid_checkpoint_fault_configuration_is_not_capacity_deferral(self):
        server = self.server("--checkpoint-interval", 1, "--save-target-ms", 3600000,
            "--save-max-ms", 7200000, "--save-idle-ms", 0, scenario="two-room",
            extra_env={"TOR_TEST_CHECKPOINT_BYTE_LIMIT": "invalid"})
        player, _ = self.client()
        self.assertIsNone(self.act(player, {"type": "wait"})["error"])
        saved = self.request(player, {"type": "save"})
        self.assertIsNotNone(saved["error"])
        self.assertNotIn("Checkpoint deferred", saved["error"])
        with sqlite3.connect(self.save) as db:
            self.assertEqual(db.execute("SELECT count(*) FROM checkpoint").fetchone()[0], 0)
            self.assertEqual(db.execute("SELECT max(sequence) FROM journal").fetchone()[0], 0)
        player.stop()
        server.child.kill(); server.child.wait(timeout=10)

    def test_text_save_and_spectator_survive_checkpoint_deferral(self):
        server = self.capacity_server(reduced=True)
        player, _ = self.text_client()
        spectator, initial = self.client(token=SPECTATOR_TOKEN, observe=True)
        actor = initial["state"]["observation"]["actor"]
        def waits(frame):
            return sum(entry["actor"] == actor and entry["content"].get("event") == {"type": "waited"}
                       for entry in frame.get("history", []))
        for count in range(1, 6):
            player.command("wait")
            self.frame(spectator, lambda frame: waits(frame) == count)
        self.assertIn("Done.", player.command("save"))
        saved = self.request(spectator, {"type": "observe"})
        self.assertEqual(saved["state"]["observation"]["tick"], "500")
        player.command("wait")
        self.frame(spectator, lambda frame: waits(frame) == 6)
        self.assertIn("Done.", player.command("save"))
        saved = self.request(spectator, {"type": "observe"})
        self.assertEqual(saved["state"]["observation"]["tick"], "600")
        spectator.stop(); player.stop()
        server.child.kill(); server.child.wait(timeout=10)
        self.capacity_server()
        player, welcome = self.text_client()
        self.assertIn("tick 600", welcome)
        spectator, restored = self.client(token=SPECTATOR_TOKEN, observe=True)
        self.assertDisclosedStateEqual(restored["state"], saved["state"])
        self.assertEqual(restored["history"], saved["history"])

    def test_full_limit_many_actor_play_crosses_retention_window_and_restarts(self):
        # A lawful synthetic workload, not the unavailable private Menagerie
        # package or player save. All 26 AI actors are peaceful and exploratory.
        package = creature_package(self, "checkpoint-many-actors", abilities=())
        region = package / "regions/1.toml"
        source = region.read_text(encoding="utf-8").replace("size = [6, 3, 2]", "size = [6, 12, 2]")
        template = source[source.index("[[actors]]"):]
        self.assertIn("id = 2\n", template)
        self.assertIn("at = [2, 1, 0]", template)
        for index in range(25):
            actor = index + 3
            extra = template.replace("id = 2\n", f"id = {actor}\n", 1)
            extra = extra.replace("at = [2, 1, 0]", f"at = [{index % 5}, {4 + index // 5}, 0]", 1)
            source += "\n" + extra
        authored = tomllib.loads(source)
        self.assertEqual(authored["size"], [6, 12, 2])
        self.assertEqual([actor["id"] for actor in authored["actors"]], list(range(2, 28)))
        positions = [tuple(actor["at"]) for actor in authored["actors"]]
        self.assertEqual(len(set(positions)), 26)
        self.assertNotIn((1, 1, 0), positions, "AI actors must not overlap the player")
        self.assertTrue(all(0 <= x < 6 and 0 <= y < 12 and z == 0 for x, y, z in positions))
        region.write_text(source, encoding="utf-8", newline="\n")
        validation = subprocess.run([self.bin / ("tor-scenario" + self.suffix), "validate", package],
            capture_output=True, text=True, encoding="utf-8", timeout=30)
        self.assertEqual(validation.returncode, 0, validation.stderr)
        server = self.server("--checkpoint-interval", 32, "--save-target-ms", 3600000,
            "--save-max-ms", 7200000, "--save-idle-ms", 0, scenario=package)
        player, _ = self.client()
        for index in range(132):
            completed = self.act(player, {"type": "wait"})
            self.assertIsNone(completed["error"])
            if (index + 1) % 32 == 0:
                # A queued SaveBehind reply is not durable completion. Use the
                # ordinary explicit-save barrier before reading a checkpoint.
                self.flush_save()
        self.flush_save()
        checkpoint = self.read_checkpoint()
        self.assertGreaterEqual(len(checkpoint["boundaries"]), 128)
        self.assertLessEqual(len(checkpoint["boundaries"]), 256)
        self.assertEqual(len(checkpoint["shared"]["actors"][checkpoint["game"]["actors"]]), 27)
        spectator, observed = self.client(token=SPECTATOR_TOKEN, observe=True)
        self.assertEqual(observed["history"], completed["history"])
        spectator.stop(); player.stop(); server.stop()
        self.server(scenario=package, seed=None)
        player, restored = self.client()
        self.assertDisclosedStateEqual(restored["state"], completed["state"])
        self.assertEqual(restored["history"], completed["history"])




class NativeCheckpointProcesses(CheckpointFixture, ProcessTestCase):
    graphical = True

    def test_text_player_and_native_ascii_spectator_resume_checkpointed_game(self):
        server = self.saving_server()
        player, _ = self.text_client()
        spectator = self.launch("tor-client-ascii", ["--connect", self.address, "--automation"],
            token=SPECTATOR_TOKEN)
        initial = self.ascii_frame(spectator, lambda f: f["state"] is not None)
        def waits(frame):
            return [entry for entry in (frame.get("history") or [])
                    if entry["actor"] == initial["state"]["observation"]["actor"]
                    and entry["content"].get("event") == {"type": "waited"}]
        for n in range(1, 10):
            player.command("wait")
            saved = self.ascii_frame(spectator, lambda f: len(waits(f)) == n)
            self.assertEqual([int(entry["tick"]) for entry in waits(saved)], list(range(0, n * 100, 100)))
            self.assertEqual(len({entry["id"] for entry in waits(saved)}), n)
        self.assertEqual(saved["state"], initial["state"], "pure waits preserve the issued picture")
        fresh = self.launch("tor-client-ascii", ["--connect", self.address, "--automation"], token=SPECTATOR_TOKEN)
        current = self.ascii_frame(fresh, lambda f: f["state"] is not None)
        self.assertEqual(current["history"], saved["history"])
        self.assertEqual(current["state"]["observation"]["tick"], "900")
        fresh.stop()
        self.assertIn("Done.", player.command("save"))
        with sqlite3.connect(self.save) as db:
            self.assertEqual(db.execute("SELECT sequence FROM checkpoint").fetchone()[0], 16)
        spectator.stop(); player.stop(); server.stop()
        self.saving_server()
        player, welcome = self.text_client()
        self.assertIn("tick 900", welcome)
        spectator = self.launch("tor-client-ascii", ["--connect", self.address, "--automation"],
            token=SPECTATOR_TOKEN)
        resumed = self.ascii_frame(spectator, lambda f: f["state"] is not None)
        for frame in (saved, resumed):
            self.assertNotIn("combat", frame["state"]["observation"])
        self.assertDisclosedStateEqual(resumed["state"], current["state"])
        self.assertEqual(resumed["history"], saved["history"])
        player.command("wait")
        advanced = self.ascii_frame(spectator, lambda f: len(waits(f)) == 10)
        self.assertEqual([int(entry["tick"]) for entry in waits(advanced)], list(range(0, 1000, 100)))
        self.assertEqual(len({entry["id"] for entry in waits(advanced)}), 10)




if __name__ == "__main__":
    unittest.main()
