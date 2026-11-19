"""Contextual developer tools through the production encoded Service."""
import json

from utility_potions_package import observation
from utility_potions_service_contracts import UtilityServiceHarness
from wizard_item_service_contracts import wizard_fixture


class WizardContextServiceContracts(UtilityServiceHarness):
    make_fixture = staticmethod(wizard_fixture)
    actors = (1, 2)

    def raw(self, connection, request, request_id=None):
        if request_id is None:
            self.serial += 1
            request_id = f"wizard-context-{self.serial}"
        self.host.request(case=self.case, connection=connection, request_id=request_id, request=request)
        return self.drain_connection(connection)

    def drain_connection(self, connection):
        return [json.loads(row) for row in self.host.call(
            "drain", case=self.case, connection=connection)["frames"]]

    def connect_role(self, role):
        connection = self.host.call("connect", case=self.case, role=role, user=role,
                                    actors=[1])["connection"]
        self.raw(connection, {"type": "attach", "actor": "1"})
        return connection

    def frame(self, connection):
        frames = self.raw(connection, {"type": "snapshot"})
        frame, = [row["snapshot"] for row in frames if row["type"] == "snapshot"]
        return frame

    def wizard_request(self, frame, operation):
        result = self.command(frame, {"type": "wait"})
        result["command"] = {"type": "wizard", "expected_revision": frame["state"]["revision"],
                             "operation": operation}
        return result

    def audit(self, user):
        return self.host.call("audit", case=self.case, actor=1, user=user)["history"]

    def report(self, wizard, frame, operation):
        rows = self.raw(wizard, self.wizard_request(frame, operation))
        self.assertEqual([row["type"] for row in rows], ["wizard_report"], rows)
        row, = rows
        self.assertEqual(row["request_id"], f"wizard-context-{self.serial}")
        self.assertEqual(row["context"]["actor"], "1")
        self.assertEqual(row["context"]["branch"], frame["branch"])
        self.assertEqual(row["context"]["revision"], frame["state"]["revision"])
        return "\n".join([row["report"]["title"], *row["report"]["lines"]])

    def assert_refused(self, rows):
        self.assertFalse([row for row in rows if row["type"] in ("ack", "wizard_report")], rows)
        error, = [row for row in rows if row["type"] == "error"]
        return error

    def test_where_tile_and_help_are_private_contextual_reads(self):
        wizard = self.connect_role("wizard")
        spectator = self.connect_role("spectator")
        control = self.snapshot()
        before = self.snapshot()
        # Explicit Snapshot issues a replacement picture and advances the
        # attachment display revision even without intervening gameplay.
        snapshot_increment = int(before["state"]["revision"]) - int(control["state"]["revision"])
        self.assertEqual(snapshot_increment, 1)
        before_history = [self.audit(user) for user in ("owner-1", "wizard")]
        frame = self.frame(wizard)

        where = self.report(wizard, frame, "where").splitlines()
        self.assertEqual(where[:3], ["Wizard location", "actor=1", "region=1 x=2 y=2 z=0"])
        here = self.report(wizard, frame, "inspect tile here").splitlines()
        self.assertIn("actor=1", here)
        self.assertIn("region=1 x=2 y=2 z=0", here)
        coin = self.report(wizard, frame, "inspect tile here offset +1 +0 +0").splitlines()
        self.assertIn("item=40", coin)
        self.assertIn("region=1 x=3 y=2 z=0", coin)
        other = self.report(wizard, frame, "inspect tile here offset +2 +0 +0").splitlines()
        self.assertIn("actor=2", other)
        self.assertIn("region=1 x=4 y=2 z=0", other)
        item_help = self.report(wizard, frame, "help item")
        self.assertIn("item spawn", item_help)
        self.assertIn("here", item_help)
        self.assertIn("identity", item_help)

        # No requester ACK/readmission and no broadcast of private content or
        # gameplay/snapshot/ready frames to other attached observers.
        for actor in self.actors:
            self.assertEqual(self.drain(actor), [])
        self.assertEqual(self.drain_connection(spectator), [])
        after = self.snapshot()
        for key in ("branch", "cursor", "readiness", "intentions", "has_control"):
            self.assertEqual(after[key], before[key])
        self.assertEqual(int(after["state"]["revision"]) - int(before["state"]["revision"]),
                         snapshot_increment)
        self.assertEqual(observation(after), observation(before))
        self.assertEqual([self.audit(user) for user in ("owner-1", "wizard")], before_history)

        for peer in (self.peers[1], spectator):
            frame = self.frame(peer)
            for operation in ("where", "inspect tile here", "help item"):
                self.assert_refused(self.raw(peer, self.wizard_request(frame, operation)))

    def test_malformed_outside_region_and_stale_reads_are_rejected(self):
        wizard = self.connect_role("wizard")
        frame = self.frame(wizard)
        shell = self.report(wizard, frame, "inspect tile here offset -3 +0 +0").splitlines()
        self.assertIn("region=1 x=-1 y=2 z=0", shell)
        self.assertIn("terrain=wall", shell)
        for operation, hint in (
            ("where surplus", "wizard where"),
            ("inspect tile here offset +1 +0", "expected three signed offsets"),
            ("inspect tile here offset +2147483648 +0 +0", "outside the coordinate range"),
            ("inspect tile here offset +9 +0 +0", "within 8 cells"),
            ("inspect tile here offset -4 +0 +0", "outside the loaded region"),
            ("help nonexistent-topic", "wizard help"),
        ):
            with self.subTest(operation=operation):
                error = self.assert_refused(self.raw(wizard, self.wizard_request(frame, operation)))
                self.assertIn(hint, error["message"].lower())
        stale = self.wizard_request(frame, "where")
        stale["command"]["expected_revision"] = str(int(frame["state"]["revision"]) + 1)
        error = self.assert_refused(self.raw(wizard, stale))
        self.assertRegex(error["message"].lower(), r"refresh|stale")
        self.report(wizard, self.frame(wizard), "where")

    def test_relative_spawn_matches_absolute_cell_and_stale_retry_does_not_duplicate(self):
        wizard = self.connect_role("wizard")
        before = observation(self.snapshot())
        known_ids = {row["item"]["id"] for row in before["ground_items"]}
        coin, = [row for row in before["ground_items"] if row["item"]["known_currency"] == "gold"]
        frame = self.frame(wizard)
        operation = "item spawn here offset +1 +0 +0 identity item.potion.hallucination"
        request = self.wizard_request(frame, operation)
        request_id = "relative-item-spawn"
        first = self.raw(wizard, request, request_id)
        self.assertFalse([row for row in first if row["type"] == "error"], first)
        for actor in self.actors:
            self.drain(actor)
        after = observation(self.snapshot())
        spawned, = [row for row in after["ground_items"] if row["item"]["id"] not in known_ids]
        self.assertEqual(spawned["position"], coin["position"])
        self.assertEqual(spawned["item"]["class"], "potion")
        self.assertFalse(spawned["item"]["identified"])
        self.assertEqual(after["tick"], before["tick"])
        current = self.frame(wizard)
        tile = self.report(wizard, current, "inspect tile here offset +1 +0 +0")
        item_lines = [line for line in tile.splitlines() if line.startswith("item=")]
        self.assertEqual(len(item_lines), 2)
        self.assertIn("item=40", item_lines)
        [spawned_line] = [line for line in item_lines if line != "item=40"]
        self.assertTrue(spawned_line.removeprefix("item=").isdecimal())
        self.assertNotIn(spawned["item"]["id"], tile)  # ordinary opaque handle is not a wizard ID

        # Retry the exact old envelope (including its request ID), after the
        # authoritative mutation replaced the attachment picture. It must not
        # recalculate 'here' against the new context or place an extra item.
        self.assert_refused(self.raw(wizard, request, request_id))
        # A read does not readmit a prior mutation or expose a private report
        # under its durable request ID, even with the current attachment frame.
        error = self.assert_refused(self.raw(wizard, self.wizard_request(current, "where"), request_id))
        self.assertEqual(error["code"], "request_conflict")
        self.assertEqual(observation(self.snapshot()), after)

    def test_item_catalog_paginates_exact_recipes_without_discovering_them(self):
        wizard = self.connect_role("wizard")
        spectator = self.connect_role("spectator")
        control = self.snapshot()
        before = self.snapshot()
        history = [self.audit(user) for user in ("owner-1", "wizard")]
        frame = self.frame(wizard)
        command = "item list"
        all_recipes = {}
        for _ in range(32):
            title, *lines, footer = self.report(wizard, frame, command).splitlines()
            self.assertEqual(title, "Wizard item catalog")
            self.assertEqual(len(lines) % 3, 0)
            rows = []
            for recipe, identity, appearance in zip(lines[0::3], lines[1::3], lines[2::3], strict=True):
                self.assertTrue(recipe.startswith("recipe="))
                self.assertTrue(identity.startswith("identity=item."))
                self.assertTrue(appearance.startswith("appearance="))
                key = recipe.removeprefix("recipe=")
                self.assertNotIn(key, all_recipes)
                all_recipes[key] = (identity, appearance)
                rows.append(key)
            self.assertLessEqual(len(rows), 16)
            if footer == "End of item catalog.":
                break
            self.assertEqual(len(rows), 16)
            command = f"item list after {rows[-1]}"
            self.assertEqual(footer, f"Next: wizard {command}")
        else:
            self.fail("Item catalog did not terminate")
        self.assertEqual(list(all_recipes), sorted(all_recipes))
        self.assertGreater(len(all_recipes), 16)
        self.assertEqual(all_recipes["rogue.gold"], ("identity=item.coin.gold", "appearance=item.coin"))
        self.assertEqual(all_recipes["rogue.hallucination"],
                         ("identity=item.potion.hallucination", "appearance=item.potion.saffron"))
        for actor in self.actors:
            self.assertEqual(self.drain(actor), [])
        self.assertEqual(self.drain_connection(spectator), [])
        after = self.snapshot()
        for key in ("branch", "cursor", "readiness", "intentions", "has_control"):
            self.assertEqual(after[key], before[key])
        self.assertEqual(int(after["state"]["revision"]) - int(before["state"]["revision"]),
                         int(before["state"]["revision"]) - int(control["state"]["revision"]))
        self.assertEqual(observation(after), observation(before))
        self.assertEqual([self.audit(user) for user in ("owner-1", "wizard")], history)

    def test_item_inspect_reports_real_mechanics_and_placement_privately(self):
        wizard = self.connect_role("wizard")
        frame = self.frame(wizard)
        coin = self.report(wizard, frame, "item inspect 40").splitlines()
        for line in ("Wizard item instance", "item=40", "recipe=rogue.gold", "identity=item.coin.gold",
                     "name=gold coin", "appearance=item.coin", "quantity=7", "charges=(none)",
                     "cursed=false protected=false corrosion=0",
                     "hit-enchantment=0 damage-enchantment=0 potency=0",
                     "ground region=1 x=3 y=2 z=0"):
            self.assertIn(line, coin)

        rows = self.raw(wizard, self.wizard_request(frame, "item spawn here recipe rogue.wand_invisibility"))
        self.assertFalse([row for row in rows if row["type"] == "error"], rows)
        for actor in self.actors:
            self.drain(actor)
        visible = self.snapshot()
        [wand] = [row["item"] for row in observation(visible)["ground_items"] if row["item"]["class"] in ("wand", "staff")]
        self.assertFalse(wand["identified"])
        self.assertNotEqual(wand["identity"], "item.wand.wand-invisibility")
        frame = self.frame(wizard)
        tile = self.report(wizard, frame, "inspect tile here").splitlines()
        [exact] = [line.removeprefix("item=") for line in tile if line.startswith("item=")]
        operation = f"item inspect {exact}"
        ground = self.report(wizard, frame, operation).splitlines()
        for line in (f"item={exact}", "recipe=rogue.wand_invisibility", "identity=item.wand.wand-invisibility",
                     "name=staff of invisibility", "quantity=1", "ground region=1 x=2 y=2 z=0",
                     f"appearance={wand['asset']}", f"appearance-name={wand['appearance']}"):
            self.assertIn(line, ground)
        [charges] = [int(line.removeprefix("charges=")) for line in ground if line.startswith("charges=")]
        # Independently authored Rogue invisibility device recipe: 3–7 charges.
        self.assertIn(charges, range(3, 8))
        self.assertEqual(observation(self.snapshot()), observation(visible))
        actual = self.act(1, {"type": "take", "item": wand["id"], "quantity": "1"})
        [carried] = [row for row in observation(actual)["inventory"] if row["id"] == wand["id"]]
        self.assertFalse(carried["identified"])
        self.drain_connection(wizard)
        inventory = self.report(wizard, self.frame(wizard), operation).splitlines()
        self.assertEqual(set(inventory), (set(ground) - {"ground region=1 x=2 y=2 z=0"}) | {"carried-by=1"})
        self.assertEqual(observation(self.snapshot()), observation(actual))

    def test_item_reads_reject_ordinary_access_stale_context_and_invalid_ids(self):
        wizard = self.connect_role("wizard")
        spectator = self.connect_role("spectator")
        for peer in (self.peers[1], spectator):
            frame = self.frame(peer)
            for operation in ("item list", "item inspect 40"):
                self.assert_refused(self.raw(peer, self.wizard_request(frame, operation)))
        frame = self.frame(wizard)
        for operation in ("item list", "item inspect 40"):
            stale = self.wizard_request(frame, operation)
            stale["command"]["expected_revision"] = str(int(frame["state"]["revision"]) + 1)
            self.assertEqual(self.assert_refused(self.raw(wizard, stale))["code"], "stale_context")
        for operation in ("item inspect 0", "item inspect -1", "item inspect 18446744073709551615",
                          "item inspect 1234567890123456789", "item list after"):
            self.assert_refused(self.raw(wizard, self.wizard_request(frame, operation)))
        self.assertIn("item=40", self.report(wizard, frame, "item inspect 40").splitlines())

    def test_resource_restore_and_fill_clamp_privately_for_self_and_other_actor(self):
        wizard = self.connect_role("wizard")
        spectator = self.connect_role("spectator")
        before = {actor: observation(self.snapshot(actor)) for actor in self.actors}
        player_history = {actor: self.host.call("audit", case=self.case, actor=actor,
                                                user=f"owner-{actor}")["history"]
                          for actor in self.actors}
        for command, source, target in (
            ("resource restore self stamina 4294967295", "Stamina", 1),
            ("resource fill 2 stamina", "Stamina", 2),
        ):
            rows = self.raw(wizard, self.wizard_request(self.frame(wizard), command))
            self.assertFalse([row for row in rows if row["type"] == "error"], rows)
            receipt, = [row["receipt"] for row in rows if row["type"] == "ack"]
            self.assertEqual(receipt["type"], "immediate")
            self.assertIn(f"Restored 0 {source} to actor {target}.", json.dumps(self.audit("wizard")))
            for actor in self.actors:
                observer_rows = self.drain(actor)
                self.assertNotIn(f"Restored 0 {source}", json.dumps(observer_rows))
            self.assertNotIn(f"Restored 0 {source}", json.dumps(self.drain_connection(spectator)))

        for actor in self.actors:
            self.assertEqual(observation(self.snapshot(actor)), before[actor])
            self.assertEqual(self.host.call("audit", case=self.case, actor=actor,
                                            user=f"owner-{actor}")["history"], player_history[actor])

    def test_resource_rejects_unsupported_amounts_pools_targets_and_roles_atomically(self):
        wizard = self.connect_role("wizard")
        spectator = self.connect_role("spectator")
        role_frames = [(peer, self.frame(peer)) for peer in (self.peers[1], spectator)]
        control = self.snapshot()
        before = self.snapshot()
        history = [self.audit(user) for user in ("owner-1", "wizard")]
        for peer, frame in role_frames:
            self.assert_refused(self.raw(peer, self.wizard_request(frame, "resource fill self stamina")))
        frame = self.frame(wizard)
        for command in (
            "resource restore self stamina 0", "resource restore self stamina -1",
            "resource restore self stamina 4294967296", "resource restore self stamina",
            "resource restore self unknown 1", "resource fill self health",
            "resource fill self nutrition", "resource set self stamina 1",
            "resource drain self stamina 1", "resource fill 0 stamina",
            "resource fill 987654321 stamina",
        ):
            with self.subTest(command=command):
                error = self.assert_refused(self.raw(wizard, self.wizard_request(frame, command)))
                self.assertTrue(error["message"].strip())
        for actor in self.actors:
            self.assertEqual(self.drain(actor), [])
        self.assertEqual(self.drain_connection(spectator), [])
        after = self.snapshot()
        for key in ("branch", "cursor", "readiness", "intentions", "has_control"):
            self.assertEqual(after[key], before[key])
        self.assertEqual(int(after["state"]["revision"]) - int(before["state"]["revision"]),
                         int(before["state"]["revision"]) - int(control["state"]["revision"]))
        self.assertEqual(observation(after), observation(before))
        self.assertEqual([self.audit(user) for user in ("owner-1", "wizard")], history)
