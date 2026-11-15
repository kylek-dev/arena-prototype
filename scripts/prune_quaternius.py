#!/usr/bin/env python3
"""Prune redundant exports, never models needed by glTF. Dry-run by default.

--apply --archive PATH first archives every removed/modified file outside assets.
Race assets remain local, under QAL, rather than being treated as unusable.
"""

import argparse
import hashlib
import json
from pathlib import Path
import tarfile


ROOT = Path('assets/quaternius')


def plan():
    files = sorted(p for p in ROOT.rglob('*') if p.is_file())
    references = set()
    repairs = {}
    for path in files:
        if path.suffix != '.gltf':
            continue
        document = json.loads(path.read_text())
        changed = False
        for entry in document.get('buffers', []) + document.get('images', []):
            uri = entry.get('uri')
            if not uri:
                continue
            dependency = path.parent / uri
            if not dependency.is_file():
                corrected = uri.replace('_png.png', '.png')
                dependency = path.parent / corrected
                if not dependency.is_file():
                    raise ValueError(f'Missing glTF dependency: {path}: {uri}')
                entry['uri'] = corrected
                changed = True
            references.add(dependency)
        if changed:
            repairs[path] = json.dumps(document, indent=2) + '\n'
    remove = {}
    for path in files:
        if path in references:
            continue
        if path.suffix.lower() in ('.fbx', '.obj', '.mtl', '.blend'):
            remove[path] = 'redundant non-glTF export'
        elif path.name.endswith('_RM.glb'):
            remove[path] = 'root-motion variant (simulation owns movement)'
        elif ('Unreal Normal Maps' in path.parts or
              'Normals-UnrealEngine' in path.parts):
            remove[path] = 'DirectX/Unreal normals; retain glTF/OpenGL normals'
        elif path.name.endswith('_Setup.png') or path.name.startswith('Importing_'):
            remove[path] = 'other-engine import screenshot'
    # Keep textures used by exports, plus unique alternates for future authoring.
    hashes = {}
    for path in files:
        if path.suffix.lower() != '.png' or path in remove:
            continue
        # Deduplicate within a pack, never across license boundaries.
        pack = path.relative_to(ROOT).parts[0]
        key = (pack, hashlib.sha256(path.read_bytes()).hexdigest())
        hashes.setdefault(key, []).append(path)
    for group in hashes.values():
        referenced = [p for p in group if p in references]
        keep = set(referenced or [min(group, key=lambda p: (len(str(p)), str(p)))])
        for path in group:
            if path not in keep:
                remove[path] = 'byte-identical unreferenced texture copy'
    return remove, repairs


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--apply', action='store_true')
    parser.add_argument('--archive', type=Path)
    args = parser.parse_args()
    remove, repairs = plan()
    total = sum(p.stat().st_size for p in remove)
    print(f'{len(remove)} redundant files, {total / 1024**2:.1f} MiB; '
          f'{len(repairs)} glTF URI repairs')
    for path, reason in remove.items():
        print(f'REMOVE {path}: {reason}')
    for path in repairs:
        print(f'REPAIR {path}')
    if not args.apply:
        return
    if not args.archive or args.archive.exists():
        parser.error('--apply requires a new --archive PATH')
    if args.archive.resolve().is_relative_to(ROOT.resolve()):
        parser.error('Archive must be outside the asset library')
    args.archive.parent.mkdir(parents=True, exist_ok=True)
    # Exclusive creation: never overwrite a prior backup.
    with tarfile.open(args.archive, 'x:gz') as archive:
        for path in sorted(set(remove) | set(repairs)):
            archive.add(path, arcname=str(path), recursive=False)
    for path in remove:
        path.unlink()
    for path, contents in repairs.items():
        path.write_text(contents)
    print(f'Applied. Recoverable originals: {args.archive}')


if __name__ == '__main__':
    main()
