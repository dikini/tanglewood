#!/usr/bin/env python3
"""Adapt pinned upstream preparation and publish explicit derived image metadata."""
import json
import os
import pathlib
import subprocess
import sys

if len(sys.argv) != 2 or sys.argv[1] not in ('json', 'html'):
    raise SystemExit('expected Pandoc writer target json or html')
source = pathlib.Path(os.environ['TANGLEWOOD_P0_SOURCE'])
result = subprocess.run(
    [sys.executable, '-m', 'illustration_md.prepare', '--source', str(source)],
    input=sys.stdin.buffer.read(), capture_output=True,
)
sys.stderr.buffer.write(result.stderr)
if result.returncode:
    raise SystemExit(result.returncode)
tree = json.loads(result.stdout)


def nodes(value, kind):
    if isinstance(value, dict):
        if value.get('t') == kind:
            yield value
        for child in value.values():
            yield from nodes(child, kind)
    elif isinstance(value, list):
        for child in value:
            yield from nodes(child, kind)


# Resolve paths in the processor, from its explicit image nodes. The renderer
# consumes this map instead of knowing the upstream asset/sidecar conventions.
images = {}
for figure in nodes(tree['blocks'], 'Figure'):
    identifier = figure['c'][0][0]
    rendered = list(nodes(figure['c'][2], 'Image'))
    if not identifier or identifier in images or len(rendered) != 1:
        raise SystemExit('expected unique figure ID and one rendered image')
    image = rendered[0]['c'][2][0]
    extension = pathlib.PurePosixPath(image).suffix.lower()
    media_type = {'.svg': 'image/svg+xml', '.png': 'image/png'}.get(extension)
    if media_type is None:
        raise SystemExit('unsupported rendered image type')
    images[identifier] = {'t': 'MetaMap', 'c': {
        'image': {'t': 'MetaString', 'c': image},
        'media-type': {'t': 'MetaString', 'c': media_type},
    }}
tree['meta']['illustrations'] = {'t': 'MetaMap', 'c': images}
output = json.dumps(tree, ensure_ascii=False).encode('utf-8')
pathlib.Path(os.environ['TANGLEWOOD_P0_OUTPUT'] + '.' + sys.argv[1] + '.prepared.json').write_bytes(output)
sys.stdout.buffer.write(output)
