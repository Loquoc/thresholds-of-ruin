"""Build disclosed cells and observations for the production and reference policy tests."""

AMULET = "Amulet of Yendor"


def cell(key, x=0, y=0, *, wall=False, down=False, up=False):
    return {"key": key, "position": {"x": x, "y": y, "z": 0}, "wall": wall,
            "stairs_down": down, "stairs_up": up}


def state(cells, *, actors=(), hostiles=(), abilities=(), inventory=(), ground=()):
    return {"observation": {
        "ready": True, "self_target": "player", "visible_cells": cells,
        "visible_actors": actors, "inventory": inventory, "ground_items": ground,
        "combat": {"hp": "10", "max_hp": "10", "terminal": False, "dead": False,
                   "preparation_remaining": None,
                   "actors": [{"actor": actor, "hostile": True} for actor in hostiles],
                   "own_stats": {"abilities": abilities, "nutrition": {"hunger": "normal"}}},
    }}
