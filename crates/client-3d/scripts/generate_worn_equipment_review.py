"""Write bounded real-render worn-equipment review scripts, never screenshots.

Render a generated JSON with the native client using
--scene-script FILE --capture NEW_DIRECTORY --offscreen.
"""
import argparse
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[3]


def catalog():
    source = (ROOT / 'crates/client-3d/model/src/wearables.rs').read_text()
    names = source.split('pub const fn name', 1)[1].split('pub const fn slot', 1)[0]
    return re.findall(r'Self::\w+ => "([^"]+)"', names)


def actor(name, index, equipment, *, woman=None, model='Human', outfit='native'):
    return {'id': name, 'model': model, 'seed': 17 + index,
            'body': {'woman': index % 2 == 0 if woman is None else woman,
                     'build': index % 3, 'bust': 2, 'figure': index % 3},
            'outfit': outfit, 'equipment': equipment,
            'position': [(index % 4 - 1.5) * 2.2, -0.5, (index // 4) * 2.7], 'yaw': 0}


def sheet(actors, name):
    events = []
    for a in actors:
        for tick, operation in [(1, {'type': 'velocity', 'value': [0.4, 0, 0]}),
                                (40, {'type': 'velocity', 'value': [0, 0, -0.4]}),
                                (80, {'type': 'velocity', 'value': [0, 0, 0]}),
                                (85, {'type': 'action', 'name': 'strike'})]:
            events.append({'tick': tick, 'actor': a['id'], 'operation': operation})
    result = {'version': 1, 'hz': 50, 'viewport': [1920, 1280], 'actors': actors,
            'floor_color': [0.25, 0.27, 0.30],
            'cameras': [{'id': 'front', 'position': [0, 4.2, 10.5], 'target': [0, 0.7, 1.2], 'fov_degrees': 48},
                        {'id': 'rear', 'position': [0, 3.8, -8], 'target': [0, 0.7, 1.2], 'fov_degrees': 48}],
            'events': sorted(events, key=lambda e: e['tick']),
            'captures': [{'tick': tick, 'name': f'{name}-{pose}', 'camera': camera}
                         for tick, pose, camera in [(0, 'front', 'front'), (0, 'rear', 'rear'),
                                                    (28, 'walking', 'front'), (65, 'turning', 'rear'),
                                                    (93, 'striking', 'front')]]}
    # Full sheets establish combinations; individual cameras expose seams,
    # glove cuffs and footwear that are too small to judge on a contact sheet.
    closeups = []
    for a in actors:
        x, y, z = a['position']
        camera = f'detail-{a["id"]}'
        result['cameras'].append({'id': camera, 'position': [x + 0.35, y + 1.15, z + 3.1],
                                  'target': [x, y + 0.8, z], 'fov_degrees': 38})
        closeups.append({'tick': 0, 'name': f'{name}-{camera}', 'camera': camera})
    result['captures'][2:2] = closeups
    return result


def reviews():
    names = catalog()
    assert len(names) == len(set(names)) == 39
    pages = {}
    for start in range(0, len(names), 8):
        page = f'appearances-{start // 8 + 1:02}'
        pages[page] = sheet([actor(name, i, [name]) for i, name in enumerate(names[start:start + 8])], page)
    layers = [
        actor('shirt-plate-open-cape', 0, ['hawaiian-shirt', 'plate-mail', 'opera-cloak']),
        actor('tshirt-chain-cloak', 1, ['t-shirt', 'chain-mail', 'hooded-cloak']),
        actor('robe-over-full-skirt', 2, ['robe', 'padded-gloves'], woman=True, outfit='royal-princess'),
        actor('apron-over-caster-robe', 3, ['apron', 'riding-gloves'], model='Mage'),
        actor('centaur-head-and-hands', 4, ['fedora', 'fencing-gloves'], model='Centaur'),
        actor('cope-and-armor', 5, ['ornamental-cope', 'plate-mail']),
        actor('wrapping-over-shirt', 6, ['mummy-wrapping', 't-shirt']),
        actor('leather-mantle-over-shirt', 7, ['leather-cloak', 'hawaiian-shirt']),
    ]
    pages['layering'] = sheet(layers, 'layering')
    return pages


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    pages = reviews()
    for name, script in pages.items():
        (args.output / f'{name}.json').write_text(json.dumps(script, indent=2) + '\n')
    index = [{'page': page, 'actor': a['id'], 'equipment': a['equipment'], 'body': a['body']}
             for page, script in pages.items() for a in script['actors']]
    (args.output / 'index.json').write_text(json.dumps(index, indent=2) + '\n')
    print(f'Wrote {len(pages)} scripts / {sum(len(s["captures"]) for s in pages.values())} captures; images not rendered.')


if __name__ == '__main__':
    main()
