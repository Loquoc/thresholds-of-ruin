# Rogue item model coverage

This matrix covers both `scenarios/rogue` and `scenarios/rogue-exploration`,
including the tuned Rogue scenario integrated by PR #219 (main `522e07af`).
It is a **catalog-to-recipe contract**, not evidence that every entry has been
rendered in ordinary gameplay. Rendering evidence is recorded separately.

The birth-inventory milestone adds one intentional generated-arrow alias sharing
the original arrow identity, appearance and shape. Earlier rendered contact-sheet
receipts cover 85 definitions / 99 forms; they are not relabeled as 86 / 100.
The added alias changes generation policy, not geometry.

## Scope and disclosure

- **86 catalog definitions, 83 identity groups**, **100 physical forms**: fourteen device identities
  each have a wand and staff form; the other 72 definitions have one form. Starter/generated arrow, mace and ring-mail
  definitions deliberately share their respective identities.
- **74 persistent appearance labels** in five pools: 14 potions, 18 scrolls,
  14 metal wands, 14 wooden staves, and 14 gemstone rings.
- **28 base shape recipes**. Shared category shapes are deliberate; appearance
  surface variants refine their disclosed color/material/markings rather than
  revealing magical identity, enchantment, charges, curse, or value.
- Item `appearance` is human-readable text; the existing wire `asset` is the
  persistent machine-readable appearance key, such as `item.potion.amber`.
  Both stay unchanged after identification. Surface recipes use the asset key,
  never display `name`, internal identity, or localized appearance text.
- Known identity is a separate semantic hierarchy from appearance, template
  archetype, and opaque instance target. Unknown identity is represented by a
  reserved public-class sentinel, not a prefix inferred from a secret identity.
- Worn equipment exposes only public cosmetic information. Sampled object
  presentations use their own cosmetic asset/kind, never the underlying item.
- Hierarchical appearance fallback walks complete dot-separated segments:
  `item.potion.amber.cloudy` may use the amber recipe; an unknown color falls
  back to the neutral potion recipe. Missing palette authority stays neutral.
- Authored appearance records explicitly pair `{ name, asset }`; no runtime
  slugging of human labels or compatibility decoding of old string entries.

The independent expected matrix is
[`scripts/fixtures/rogue-item-model-coverage.json`](../../scripts/fixtures/rogue-item-model-coverage.json),
checked against both live catalogs and the resolver by
[`test_rogue_item_model_coverage.py`](../../scripts/test_rogue_item_model_coverage.py).
The catalogs remain byte-identical. The initial cosmetic-key change was checked
against the PR #219 integration: stripping only `asset` fields leaves all parsed
catalog data equal. No rule, quantity, probability or item effect was changed.

## Ground and equipped presentation

Every row has a ground shape recipe. Weapon-slot weapons and devices also use
the disclosed recipe for held presentation; body armor uses the worn armor
path. Rings and other small worn accessories are not disclosed in `WornView`.
Torch ground art is covered, but the `held_light` attachment is **not** covered by
the weapon attachment path. Catalog coverage must not imply otherwise.

Tuned Rogue currently references 84 catalog definitions in its live manifest
(clean bandage and torch are unreferenced); exploration references 85 (torch is
unreferenced). Both full catalogs remain covered, including opt-in items.
Corpses are generated from actors rather than item catalog definitions. Tuned
Rogue disables corpses; exploration uses disclosed creature-body rendering and
an anonymous fallback, outside this 86-definition matrix.

## Catalog matrix

`A / B` denotes the two physical forms, not two copies of the identity.
Pool assets in the table denote base families; their exact leaf keys are listed
below. All assets are public cosmetic keys. Identified and unidentified magical
items retain their physical form and appearance-surface selection.

| Catalog identity | Public asset(s) | Shape recipe(s) |
|---|---|---|
| `rogue.mace` | `item.weapon.mace` | `Mace` |
| `rogue.ring_mail` | `item.armor.mail.ring` | `RingMail` |
| `rogue.clean_bandage` | `item.bandage` | `Bandage` |
| `rogue.ration` | `item.food.ration` | `Ration` |
| `rogue.fruit` | `item.food.fruit` | `Fruit` |
| `rogue.amulet_of_yendor` | `item.amulet` | `Amulet` |
| `rogue.restore_strength` | `item.potion` | `Potion` |
| `rogue.gold` | `item.coin` | `Coins` |
| `rogue.healing` | `item.potion` | `Potion` |
| `rogue.extra_healing` | `item.potion` | `Potion` |
| `rogue.confusion` | `item.potion` | `Potion` |
| `rogue.blindness` | `item.potion` | `Potion` |
| `rogue.poison` | `item.potion` | `Potion` |
| `rogue.see_invisible` | `item.potion` | `Potion` |
| `rogue.identify_potion` | `item.scroll` | `Scroll` |
| `rogue.sleep` | `item.scroll` | `Scroll` |
| `rogue.identify_scroll` | `item.scroll` | `Scroll` |
| `rogue.identify_wand` | `item.scroll` | `Scroll` |
| `rogue.wand_fire` | `item.wand` / `item.wand.staff` | `Wand` / `Staff` |
| `rogue.wand_cold` | `item.wand` / `item.wand.staff` | `Wand` / `Staff` |
| `rogue.wand_lightning` | `item.wand` / `item.wand.staff` | `Wand` / `Staff` |
| `rogue.wand_invisibility` | `item.wand` / `item.wand.staff` | `Wand` / `Staff` |
| `rogue.wand_haste` | `item.wand` / `item.wand.staff` | `Wand` / `Staff` |
| `rogue.wand_slow` | `item.wand` / `item.wand.staff` | `Wand` / `Staff` |
| `rogue.generated_mace` | `item.weapon.mace` | `Mace` |
| `rogue.long_sword` | `item.weapon.sword.long` | `LongSword` |
| `rogue.dagger` | `item.weapon.dagger` | `Dagger` |
| `rogue.shortbow` | `item.weapon.bow.short` | `Shortbow` |
| `rogue.arrow` | `item.weapon.arrow` | `Arrow` |
| `rogue.generated_arrow` | `item.weapon.arrow` | `Arrow` |
| `rogue.two_handed_sword` | `item.weapon.greatsword` | `Greatsword` |
| `rogue.spear` | `item.weapon.spear` | `Spear` |
| `rogue.leather_armor` | `item.armor.leather` | `LeatherArmor` |
| `rogue.generated_ring_mail` | `item.armor.mail.ring` | `RingMail` |
| `rogue.studded_leather` | `item.armor.leather.studded` | `StuddedLeather` |
| `rogue.scale_mail` | `item.armor.mail.scale` | `ScaleMail` |
| `rogue.chain_mail` | `item.armor.mail.chain` | `ChainMail` |
| `rogue.splint_mail` | `item.armor.plate.splint` | `SplintMail` |
| `rogue.banded_mail` | `item.armor.plate.banded` | `BandedMail` |
| `rogue.plate_mail` | `item.armor.plate` | `PlateMail` |
| `rogue.enchant_armor` | `item.scroll` | `Scroll` |
| `rogue.protect_armor` | `item.scroll` | `Scroll` |
| `rogue.enchant_weapon` | `item.scroll` | `Scroll` |
| `rogue.remove_curse` | `item.scroll` | `Scroll` |
| `rogue.identify_weapon` | `item.scroll` | `Scroll` |
| `rogue.identify_armor` | `item.scroll` | `Scroll` |
| `rogue.ring_protection` | `item.ring` | `Ring` |
| `rogue.ring_add_strength` | `item.ring` | `Ring` |
| `rogue.ring_sustain_strength` | `item.ring` | `Ring` |
| `rogue.ring_see_invisible` | `item.ring` | `Ring` |
| `rogue.ring_adornment` | `item.ring` | `Ring` |
| `rogue.ring_dexterity` | `item.ring` | `Ring` |
| `rogue.ring_increase_damage` | `item.ring` | `Ring` |
| `rogue.ring_regeneration` | `item.ring` | `Ring` |
| `rogue.ring_slow_digestion` | `item.ring` | `Ring` |
| `rogue.ring_maintain_armor` | `item.ring` | `Ring` |
| `rogue.ring_searching` | `item.ring` | `Ring` |
| `rogue.levitation` | `item.potion` | `Potion` |
| `rogue.teleportation` | `item.scroll` | `Scroll` |
| `rogue.ring_teleportation` | `item.ring` | `Ring` |
| `rogue.wand_teleport_away` | `item.wand` / `item.wand.staff` | `Wand` / `Staff` |
| `rogue.wand_teleport_to` | `item.wand` / `item.wand.staff` | `Wand` / `Staff` |
| `rogue.ring_stealth` | `item.ring` | `Ring` |
| `rogue.ring_aggravation` | `item.ring` | `Ring` |
| `rogue.aggravate_monster` | `item.scroll` | `Scroll` |
| `rogue.create_monster` | `item.scroll` | `Scroll` |
| `rogue.hold_monster` | `item.scroll` | `Scroll` |
| `rogue.monster_confusion` | `item.scroll` | `Scroll` |
| `rogue.scare_monster` | `item.scroll` | `Scroll` |
| `rogue.wand_nothing` | `item.wand` / `item.wand.staff` | `Wand` / `Staff` |
| `rogue.wand_light` | `item.wand` / `item.wand.staff` | `Wand` / `Staff` |
| `rogue.wand_drain_life` | `item.wand` / `item.wand.staff` | `Wand` / `Staff` |
| `rogue.wand_cancellation` | `item.wand` / `item.wand.staff` | `Wand` / `Staff` |
| `rogue.wand_magic_missile` | `item.wand` / `item.wand.staff` | `Wand` / `Staff` |
| `rogue.torch` | `item.tool.torch` | `Torch` |
| `rogue.haste` | `item.potion` | `Potion` |
| `rogue.gain_strength` | `item.potion` | `Potion` |
| `rogue.raise_level` | `item.potion` | `Potion` |
| `rogue.hallucination` | `item.potion` | `Potion` |
| `rogue.monster_detection` | `item.potion` | `Potion` |
| `rogue.food_detection` | `item.scroll` | `Scroll` |
| `rogue.magic_detection` | `item.potion` | `Potion` |
| `rogue.magic_mapping` | `item.scroll` | `Scroll` |
| `rogue.wand_polymorph` | `item.wand` / `item.wand.staff` | `Wand` / `Staff` |
| `rogue.dart` | `item.weapon.dart` | `Dart` |
| `rogue.shuriken` | `item.weapon.shuriken` | `Shuriken` |

## Persistent appearance pools

Every entry has an explicit stable machine key. Category geometry is shared,
while the surface variant follows that key. The same key may belong to different
magical identities across seeded worlds; identification never changes it.

### `rogue.potions` (14 entries)

| Human appearance | Persistent asset key |
|---|---|
| amber potion | `item.potion.amber` |
| smoky potion | `item.potion.smoky` |
| violet potion | `item.potion.violet` |
| azure potion | `item.potion.azure` |
| crimson potion | `item.potion.crimson` |
| silver potion | `item.potion.silver` |
| pearl potion | `item.potion.pearl` |
| teal potion | `item.potion.teal` |
| ochre potion | `item.potion.ochre` |
| indigo potion | `item.potion.indigo` |
| copper potion | `item.potion.copper` |
| chartreuse potion | `item.potion.chartreuse` |
| saffron potion | `item.potion.saffron` |
| cerulean potion | `item.potion.cerulean` |

### `rogue.scrolls` (18 entries)

| Human appearance | Persistent asset key |
|---|---|
| scroll titled VEL MOR | `item.scroll.vel-mor` |
| scroll titled ELAM TOR | `item.scroll.elam-tor` |
| scroll titled NAR SHEN | `item.scroll.nar-shen` |
| scroll titled HETH ARA | `item.scroll.heth-ara` |
| scroll titled TAL ION | `item.scroll.tal-ion` |
| scroll titled UR SEK | `item.scroll.ur-sek` |
| scroll titled KEL RAN | `item.scroll.kel-ran` |
| scroll titled AN DOR | `item.scroll.an-dor` |
| scroll titled IV MEM | `item.scroll.iv-mem` |
| scroll titled OS THAL | `item.scroll.os-thal` |
| scroll titled VEL TOR | `item.scroll.vel-tor` |
| scroll titled SAH REN | `item.scroll.sah-ren` |
| scroll titled OR VETH | `item.scroll.or-veth` |
| scroll titled NEM AL | `item.scroll.nem-al` |
| scroll titled TIR OS | `item.scroll.tir-os` |
| scroll titled KHA MER | `item.scroll.kha-mer` |
| scroll titled LOR NEV | `item.scroll.lor-nev` |
| scroll titled MIR VEL | `item.scroll.mir-vel` |

### `rogue.wands` (14 entries)

| Human appearance | Persistent asset key |
|---|---|
| aluminum wand | `item.wand.aluminum` |
| bronze wand | `item.wand.bronze` |
| copper wand | `item.wand.copper` |
| electrum wand | `item.wand.electrum` |
| gold wand | `item.wand.gold` |
| iron wand | `item.wand.iron` |
| lead wand | `item.wand.lead` |
| magnesium wand | `item.wand.magnesium` |
| nickel wand | `item.wand.nickel` |
| platinum wand | `item.wand.platinum` |
| silver wand | `item.wand.silver` |
| steel wand | `item.wand.steel` |
| tin wand | `item.wand.tin` |
| zinc wand | `item.wand.zinc` |

### `rogue.staves` (14 entries)

| Human appearance | Persistent asset key |
|---|---|
| ash staff | `item.wand.staff.ash` |
| birch staff | `item.wand.staff.birch` |
| cedar staff | `item.wand.staff.cedar` |
| cherry staff | `item.wand.staff.cherry` |
| elm staff | `item.wand.staff.elm` |
| hazel staff | `item.wand.staff.hazel` |
| mahogany staff | `item.wand.staff.mahogany` |
| maple staff | `item.wand.staff.maple` |
| oak staff | `item.wand.staff.oak` |
| pine staff | `item.wand.staff.pine` |
| teak staff | `item.wand.staff.teak` |
| walnut staff | `item.wand.staff.walnut` |
| willow staff | `item.wand.staff.willow` |
| yew staff | `item.wand.staff.yew` |

### `rogue.rings` (14 entries)

| Human appearance | Persistent asset key |
|---|---|
| agate ring | `item.ring.agate` |
| bronze ring | `item.ring.bronze` |
| coral ring | `item.ring.coral` |
| diamond ring | `item.ring.diamond` |
| emerald ring | `item.ring.emerald` |
| ivory ring | `item.ring.ivory` |
| jade ring | `item.ring.jade` |
| opal ring | `item.ring.opal` |
| ruby ring | `item.ring.ruby` |
| sapphire ring | `item.ring.sapphire` |
| topaz ring | `item.ring.topaz` |
| amethyst ring | `item.ring.amethyst` |
| garnet ring | `item.ring.garnet` |
| quartz ring | `item.ring.quartz` |

## Compatibility and validation boundary

The coordinated appearance/identity/condition change uses the current protocol,
scenario, validator and save versions together. Older clients and older saves
are explicitly rejected under the project's current-format-only policy; no
migration guesses identities or appearance keys from old display strings.
Existing saves cannot be silently replaced with current authored definitions:
see [save ownership](../reference/scenario-packages.md#saves-and-the-package).

The appearance-only portion preserves every non-art Rogue catalog value after
normalizing the paired records back to their original labels. The broader
identity and condition DTO changes have separate disclosure and persistence
tests. A successful static matrix check is not a substitute for protocol,
restart, ordinary-play, or screenshot validation.

### Explicit identity migration evidence

`scripts/fixtures/rogue-identity-coverage.json` records every old identity group
and its explicit new key. The test requires a bijection: aliases remain aliases,
and distinct old groups never merge. The 21 token fixtures retain their three
separate copper/silver/iron groups using paired `SeedItemName` records and the
same seeded index, rather than deriving identities from the selected display
name. Canonical key ordering may alter appearance assignments for a particular
old seed; current-format replay must remain deterministic. No historical replay
compatibility is claimed across the approved version break.

## Reproducing real-render review captures

Generate contact sheets for all 100 physical forms and all 74 public appearance
leaves with `python crates/client-3d/scripts/generate_rogue_item_review.py OUTPUT`.
The output index keeps template archetypes separate from canonical identity keys.
Run each generated scene with the actual client:

```sh
tor-client-3d --scene-script OUTPUT/all-forms-01.json --capture NEW_CAPTURE_DIR --offscreen
```

Checked-in scenes under `crates/client-3d/scripts/fixtures/rogue-items/` add light
and dark dungeon floors, wall/character clutter, and the large-pile picker at
the normal 8.5 camera distance. The two appearance-persistence scenes use the
same camera, targets, appearance keys and unlabeled layout, changing only the
known display names; their decoded pixels should match. These are deterministic
fixtures using production geometry, item recipes and UI, not a claim of a live
network gameplay session. Each capture directory must be new. A software Vulkan
adapter can run them without a display; a graphics adapter is still required.

Small groups keep individual targets; a larger cell pile opens an item picker.
Hold Alt or toggle F3 to show disclosed item labels. Visual coin pieces are
illustrative, while labels show the exact disclosed quantity. Unknown magic and
BUC do not change appearance. Ground torch models are covered; offhand/held-light
torch rendering remains outside this change.
