# Item identity, appearance, and remembered facts

An item's public appearance is not its true identity. Protocol 68 keeps them
separate, and does not derive either machine key from a translated display name.

```json
{
  "identity": "item.potion.unknown",
  "asset": "item.potion.amber",
  "appearance": "amber potion",
  "name": "amber potion",
  "identified": false,
  "knowledge": {
    "buc": "item.buc.unknown",
    "enchantments": {"hit": null, "damage": null, "armor": null, "potency": null},
    "condition": {"corrosion": null, "armor_deterioration": null, "protected": null}
  }
}
```

This is an excerpt of an item, not a complete observation or request.

## Independent hierarchies

- `identity` is a validated `ItemIdentity`, rooted at `item`. An identified healing
  potion may disclose `item.potion.healing`. An unknown potion discloses
  `item.potion.unknown`; an unspecified or cosmetically disguised object may
  disclose `item.unknown`. The sentinel is not an authored true item identity.
- `asset` is the existing public appearance key. `item.potion.amber` stays the
  same when the potion is identified, or when the same appearance belongs to a
  different identity in another run. No competing appearance-ID field is added.
- `appearance` and `name` remain human-readable labels. Identification can change
  the latter without changing the appearance key, geometry, materials, or pose.

Identity segments are lowercase ASCII letters/digits with interior single
hyphens, separated by dots, within 80 bytes. Segments start with a letter; empty
segments, underscores, whitespace, normalization aliases, and an intermediate
`unknown` segment are rejected. Hierarchy operations compare whole segments:
`item.potionx` is not a descendant of `item.potion`. The `item` root and category
nodes can be used for comparisons; authored true identities must be known leaves.

Appearance lookup also falls back through whole dotted parents. An unsupported
suffix beneath a supported finish can reuse that finish; an unknown finish can
reuse its physical category with a neutral material. It never guesses a finish
from a true identity, effect, curse, or display label. Palette authority still
applies before a live appearance is resolved.

## Structured observer knowledge

The same required `knowledge` block accompanies ground and inventory items.
Every nested field is required. JSON `null` means unknown, not zero, false, or a
negative answer. Explicit `0` and `false` are known values. Unknown fields,
missing fields, invalid BUC keys, and out-of-bound numbers are rejected.

| Field | Meaning |
| --- | --- |
| `buc` | `item.buc.unknown`, `item.buc.uncursed`, or `item.buc.cursed` |
| `enchantments.hit` | Remembered signed hit enchantment |
| `enchantments.damage` | Remembered signed damage enchantment |
| `enchantments.armor` | Remembered nonnegative armor enchantment |
| `enchantments.potency` | Remembered signed equipment-modifier potency, not charges |
| `condition.corrosion` | Remembered nonnegative corrosion |
| `condition.armor_deterioration` | Remembered nonnegative physical armor penalty |
| `condition.protected` | Remembered protection flag, independent of BUC |

Signed values range from -1,000,000 to 1,000,000; unsigned values from 0 to
1,000,000. A field does not grant a capability merely because it has a value.
Applicability and available actions remain governed by the existing item and
interaction contracts. In particular, protection does not mean blessed, and
uncursed does not mean unenchanted. No blessed state or new blessing mechanic is
introduced.

These are remembered, exact-instance facts. Existing learning events determine
what is known; observation never fills an unknown field from hidden current
condition. An unseen change does not refresh an old rust or enchantment fact.
Consequently BUC can be known while type identity is unknown, type can be known
while BUC is unknown, and remembered numbers can be known independently of type.
Worn visual projections remain appearance-only, not an equipment-inspection API.
Spectators receive `item.unknown` and unknown item-knowledge fields rather than
the followed controller's private facts. Existing public display names,
`identified` flags, quantities and currency affordances remain unchanged; a
public display label alone does not grant semantic identity knowledge. The
restriction applies to both inventory and ground rows according to the current
access role. Command-control revocation alone does not change the established
Player role's read-knowledge policy.

## Cosmetic disguises and authority

A sampled ground-object presentation redacts the new identity to `item.unknown`
and the entire new knowledge block to unknown. Its appearance comes only from
the sampled presentation, never the underlying real item's appearance. Existing
opaque targets, public class, identified flag, currency/supply affordances, and
server-owned actions keep their existing semantics. Thus known identity implies
`identified`, but `identified` does not force a true identity key to be disclosed
through a cosmetic override. The ground wrapper validates this redaction.

## Authoring and persistence

Appearance pools contain explicit `{name, asset}` records. Label and optional
asset are selected together; an explicitly assetless entry does not fall back to
an identity-specific asset. Forecasts include every possible public entry,
independently of the hidden assignment. The two Rogue catalogs use the same
appearance records. Display-name localization does not generate machine keys.

Ordinary item construction requires a typed identity separately from its name.
Corpse identity is explicitly authored on the creature persona, not derived from
its display name or current polymorphed body. Intended corpse knowledge groups
share an authored key; distinct personas do not collapse merely because they
share a species. The normal corpse disclosure gates still apply.

The combined change uses protocol 68, scenario format 37, validator 49, and save
format 66. Its internal compiled-content and region envelopes advance to 34 and
10 respectively; rules input remains 26. Older clients and saves are explicitly rejected, not silently
migrated. No gameplay numeric tuning or new knowledge-learning event is part of
this format change.
