"""Private wizard authored-item creation through the production Service encoder."""
import json
from pathlib import Path
import tomllib

from hallucination_package import fixture
from utility_potions_service_contracts import UtilityServiceHarness
from utility_potions_package import condition, observation


def wizard_fixture():
    result = fixture()
    result["wizard"] = True
    result["regions"][0]["items"] = [row for row in result["regions"][0]["items"] if row["id"] != 30]
    return result


class WizardItemHarness(UtilityServiceHarness):

    def connect_role(self, role):
        connection = self.host.call("connect", case=self.case, role=role,
                                   user=role, actors=[1])["connection"]
        self.raw(connection, {"type": "attach", "actor": "1"})
        return connection

    def raw(self, connection, request):
        self.serial += 1
        self.host.request(case=self.case, connection=connection, request_id=f"wizard-{self.serial}", request=request)
        return [json.loads(row) for row in self.host.call("drain", case=self.case, connection=connection)["frames"]]

    def spawn(self, connection, selector, coordinates="1 2 2 0"):
        return self.wizard_command(connection, f"item spawn {coordinates} {selector}")

    def wizard_command(self, connection, operation):
        frames = self.raw(connection, {"type": "snapshot"})
        frame, = [row["snapshot"] for row in frames if row["type"] == "snapshot"]
        command = self.command(frame, {"type": "wait"})
        command["command"] = {"type": "wizard", "expected_revision": frame["state"]["revision"],
                              "operation": operation}
        rows = self.raw(connection, command)
        # Wizard setup publishes replacement snapshots to existing attachments.
        # Drain those real unsolicited pictures before requesting another one.
        for actor, peer in self.peers.items():
            if peer != connection:
                self.drain(actor)
        return rows

class WizardItemServiceContracts(WizardItemHarness):
    make_fixture = staticmethod(wizard_fixture)
    actors = (1, 2)

    def test_wizard_spawn_is_private_unidentified_canonical_and_player_cannot_spawn(self):
        wizard = self.connect_role("wizard")
        before = observation(self.snapshot())
        for peer in (self.peers[1], self.connect_role("spectator")):
            rows = self.spawn(peer, "identity item.potion.hallucination")
            self.assertTrue(any(row["type"] == "error" for row in rows))
            self.assertEqual(observation(self.snapshot())["ground_items"], before["ground_items"])
        rows = self.spawn(wizard, "identity item.potion.hallucination")
        self.assertFalse([row for row in rows if row["type"] == "error"], rows)
        after = observation(self.snapshot())
        added = [row for row in after["ground_items"] if row["item"]["id"] not in
                 {old["item"]["id"] for old in before["ground_items"]}]
        self.assertEqual(len(added), 1)
        item = added[0]["item"]
        self.assertFalse(item["identified"])
        self.assertNotEqual(item["identity"], "item.potion.hallucination")
        self.assertEqual(item["class"], "potion")
        self.assertEqual(after["tick"], before["tick"])
        history = self.host.call("audit", case=self.case, actor=1, user="owner-1")["history"]
        self.assertNotIn("spawn_item", json.dumps(history))
        asset = item["asset"]
        self.assertEqual(asset, "item.potion.saffron")
        # The seeded assignment, not the whole appearance pool, selects the recipe.
        for selector in (f"appearance {asset}", f"identity item.potion.hallucination appearance {asset}"):
            rows = self.spawn(wizard, selector)
            self.assertFalse([row for row in rows if row["type"] == "error"], rows)
        stable = observation(self.snapshot())
        for selector, position in (("identity item.potion.missing", "1 3 2 0"),
                                   ("identity item.potion.hallucination appearance item.wand.gold", "1 3 2 0"),
                                   ("identity item.potion.hallucination", "1 9999 2 0")):
            rows = self.spawn(wizard, selector, position)
            self.assertTrue(any(row["type"] == "error" for row in rows))
            current = observation(self.snapshot())
            self.assertEqual(current["ground_items"], stable["ground_items"])
            self.assertEqual(current["tick"], stable["tick"])

        # The new instance has its real authored effects, not merely its label.
        taken = self.act(1, {"type": "take", "item": item["id"], "quantity": "1"})
        potion, = [row for row in observation(taken)["inventory"] if row["id"] == item["id"]]
        self.assertFalse(potion["identified"])
        drank = self.act(1, {"type": "drink", "item": potion["id"]})
        active = condition(drank, "rogue.hallucinating")
        self.assertIsNotNone(active)
        self.assertIn(int(active["remaining_ticks"]), range(80800, 89201, 100))

    def test_ambiguous_appearance_rejects_atomically_and_both_selectors_disambiguate(self):
        self.host.call("close", case=self.case)
        data = wizard_fixture()
        for entry in data["manifest"]["appearance_pools"]["rogue.potions"]["appearances"]:
            entry["asset"] = "item.potion.saffron"
        self.case = self.host.call("create", **data)["case"]
        self.peers = {1: self.connect_role("player")}
        wizard = self.connect_role("wizard")
        before = observation(self.snapshot())
        history = self.host.call("audit", case=self.case, actor=1, user="wizard")["history"]
        rows = self.spawn(wizard, "appearance item.potion.saffron")
        errors = [row for row in rows if row["type"] == "error"]
        self.assertEqual(len(errors), 1)
        self.assertIn("Ambiguous", errors[0]["message"])
        self.assertEqual(observation(self.snapshot())["ground_items"], before["ground_items"])
        self.assertEqual(self.host.call("audit", case=self.case, actor=1, user="wizard")["history"], history)
        rows = self.spawn(wizard, "identity item.potion.hallucination appearance item.potion.saffron")
        self.assertFalse([row for row in rows if row["type"] == "error"], rows)
        after = observation(self.snapshot())
        self.assertEqual(len(after["ground_items"]), len(before["ground_items"]) + 1)
        self.assertEqual(after["tick"], before["tick"])

    def test_explicit_recipe_reaches_distinct_birth_policies_without_guessing(self):
        wizard = self.connect_role("wizard")
        for identity, asset, recipes in (
            ("item.weapon.mace", "item.weapon.mace", ("rogue.mace", "rogue.generated_mace")),
            ("item.weapon.arrow", "item.weapon.arrow", ("rogue.arrow", "rogue.generated_arrow")),
            ("item.armor.ring-mail", "item.armor.mail.ring", ("rogue.ring_mail", "rogue.generated_ring_mail")),
        ):
            before = observation(self.snapshot())["ground_items"]
            rows = self.spawn(wizard, f"identity {identity} appearance {asset}")
            self.assertTrue(any("Ambiguous" in row.get("message", "") for row in rows), rows)
            self.assertEqual(observation(self.snapshot())["ground_items"], before)
            for recipe in recipes:
                rows = self.spawn(wizard, f"recipe {recipe}")
                self.assertFalse([row for row in rows if row["type"] == "error"], rows)
                rows = self.spawn(wizard, f"identity {identity} recipe {recipe} appearance {asset}")
                self.assertFalse([row for row in rows if row["type"] == "error"], rows)
        before = observation(self.snapshot())["ground_items"]
        for selector in ("recipe rogue.mace identity item.weapon.arrow",
                         "recipe rogue.mace appearance item.weapon.arrow",
                         "recipe rogue.absent", "recipe rogue.mace recipe rogue.generated_mace"):
            rows = self.spawn(wizard, selector)
            self.assertTrue(any(row["type"] == "error" for row in rows), rows)
            self.assertEqual(observation(self.snapshot())["ground_items"], before)

    def test_selector_parser_rejects_malformed_text_and_forged_json(self):
        wizard = self.connect_role("wizard")
        before = observation(self.snapshot())["ground_items"]
        bad = ["item spawn 1 2 2 0", "item spawn 1 2 2 0 identity",
               "item spawn 1 2 2 0 identity potion", "item spawn 1 2 2 0 appearance saffron",
               "item spawn 1 2 2 0 identity item.potion.hallucination identity item.potion.healing",
               "item spawn 1 2 2 0 appearance item.potion.saffron junk",
               "item spawn 1 2 2 0 quantity 2"]
        authored = {"type": "spawn_item", "position": {"region": 1, "x": 2, "y": 2, "z": 0},
                    "identity": "item.potion.hallucination", "appearance": None, "recipe": None}
        missing = dict(authored)
        del missing["appearance"]
        bad.extend([json.dumps(missing), json.dumps({**authored, "spec": {"name": "forged"}}),
                    json.dumps({**authored, "identity": None})])
        for operation in bad:
            rows = self.wizard_command(wizard, operation)
            self.assertTrue(any(row["type"] == "error" for row in rows), operation)
            self.assertEqual(observation(self.snapshot())["ground_items"], before)
        for operation in (json.dumps(authored),
                          "item spawn 1 2 2 0 appearance item.potion.saffron identity item.potion.hallucination",
                          "item token 1 2 2 0", "item tablet 1 2 2 0"):
            rows = self.wizard_command(wizard, operation)
            self.assertFalse([row for row in rows if row["type"] == "error"], rows)


def ai_give_fixture():
    package = Path(__file__).resolve().parents[1] / "scenarios/tests/ai-interactions"
    manifest = tomllib.loads((package / "scenario.toml").read_text(encoding="utf-8"))
    region = tomllib.loads((package / "regions/1.toml").read_text(encoding="utf-8"))
    # Keep the real tactical policy and actual melee fixture. Only remove the
    # preloaded medicine and its knowledge so Give is the only source of either.
    region["actors"][0]["known_identities"] = []
    region["items"] = [item for item in region["items"] if item["id"] != 20]
    return {"manifest": manifest, "regions": [region],
            "rules": (package / "rules.toml").read_text(encoding="utf-8"),
            "seed": 42, "wizard": True, "frame_bytes": 65536,
            "client_bytes": 131072, "total_bytes": 524288}


class WizardAiGiveServiceContracts(WizardItemHarness):
    make_fixture = staticmethod(ai_give_fixture)
    actors = (1,)

    def test_give_and_identification_are_actor_local_until_autonomous_healing(self):
        wizard = self.connect_role("wizard")
        observer = self.host.call("connect", case=self.case, role="wizard",
                                  user="observer", actors=[2])["connection"]
        self.raw(observer, {"type": "attach", "actor": "2"})

        def guard():
            rows = self.raw(observer, {"type": "snapshot"})
            self.assertFalse([row for row in rows if row["type"] == "error"], rows)
            frame, = [row["snapshot"] for row in rows if row["type"] == "snapshot"
                      and row.get("request_id") == f"wizard-{self.serial}"]
            return observation(frame)

        def potions(view):
            return [item for item in view["inventory"] if item["class"] == "potion"]

        def guard_actions():
            history = self.host.call("audit", case=self.case, actor=2, user="observer")["history"]
            return [row for row in history["entries"] if row["content"]["type"] == "action"]

        def setup(command):
            rows = self.wizard_command(wizard, command)
            self.assertFalse([row for row in rows if row["type"] == "error"], rows)

        def wait():
            self.act(1, {"type": "wait"})
            self.raw(wizard, {"type": "snapshot"})
            return guard()

        target, = [actor["id"] for actor in observation(self.snapshot())["visible_actors"]
                   if actor["name"] == "ruin guard"]
        healthy = guard()
        self.assertEqual(healthy["combat"]["hp"], "30")
        self.assertEqual(potions(healthy), [])
        self.act(1, {"type": "use_ability", "id": "basic_melee",
                     "invocation": {"aim": {"kind": "actor_target", "target": target},
                                    "weapon": {"kind": "default"}}, "supplies": {"kind": "carried"}})
        wounded = guard()
        self.assertEqual(wounded["combat"]["hp"], "15")
        self.assertEqual(potions(wounded), [])

        setup("item give 2 identity item.potion.healing")
        given = guard()
        potion, = potions(given)
        self.assertEqual(given["tick"], wounded["tick"])
        self.assertEqual(potion["quantity"], "1")
        self.assertFalse(potion["identified"])
        self.assertNotEqual(potion["identity"], "item.potion.healing")
        # The wizard command takes the authoritative instance ID, which is
        # disclosed privately in its result (not the viewer-salted inventory ID).
        audit = self.host.call("audit", case=self.case, actor=1, user="wizard")
        summary, = [row["content"]["summary"] for row in audit["history"]["entries"]
                    if row["content"].get("summary", "").startswith("Gave item ")]
        prefix, suffix = "Gave item ", " to actor 2."
        self.assertTrue(summary.endswith(suffix), summary)
        instance = int(summary[len(prefix):-len(suffix)])

        setup(f"identify 1 {instance}")
        prior_actions = {row["id"] for row in guard_actions()}
        for _ in range(4):
            after = wait()
            self.assertEqual(after["combat"]["hp"], "15")
            self.assertEqual(potions(after), [potion])
        self.assertGreater(int(after["tick"]), int(given["tick"]))
        unknown_actions = guard_actions()
        self.assertTrue(any(row["id"] not in prior_actions for row in unknown_actions),
                        "the guard must get actual turns while the dose is unknown")
        self.assertNotIn("drink", [row["content"]["action"]["type"] for row in unknown_actions])

        setup(f"identify 2 {instance}")
        identified, = potions(guard())
        self.assertEqual(identified["id"], potion["id"])
        self.assertEqual(identified["identity"], "item.potion.healing")
        self.assertTrue(identified["identified"])
        for _ in range(5):
            after = wait()
            if not potions(after):
                break
        self.assertEqual(potions(after), [], "guard must autonomously consume the exact given dose")
        self.assertEqual(after["combat"]["hp"], "25")
        drink, = [row for row in guard_actions() if row["content"]["action"]["type"] == "drink"]
        self.assertEqual(drink["actor"], "2")
        self.assertEqual(drink["author"], {"type": "backend", "component": "scheduler"})
        self.assertEqual(drink["content"]["action"]["item"], potion["id"])
