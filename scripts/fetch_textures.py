#!/usr/bin/env python3
"""Download pinned CC0 texture sources, verify them and rebuild assets/textures.

Only needed to change or regenerate the committed runtime textures; ordinary
builds copy assets/textures/*.png and never touch the network. The same run
repaints the original sky cubemap (sky.png), which needs no download. Sources are
hand-painted CC0 sets from 3DTextures.me (https://3dtextures.me/about/), served
through the Google Drive folders linked from each texture's page.
"""

import hashlib
from pathlib import Path
import subprocess
import sys
import urllib.request

ROOT = Path(__file__).resolve().parent.parent
CACHE = ROOT / 'assets/textures/source'  # Gitignored raw downloads.
OUTPUT = ROOT / 'assets/textures'
# (file name, Google Drive file id, SHA-256). Pages: see assets/textures/README.md.
SOURCES = (
    ('Stylized_Stone_Floor_010_basecolor.png', '18JNKloxYlYAepUjkNGAltqSdsB6tnGO8',
     '08cc795639346b538b02c93e394d4b250501d7bd67c82be571831056f4fb7ec2'),
    ('Stylized_Stone_Floor_010_ambientOcclusion.png', '11Aue2GxTt1dVwPMEucrTN8IvaFbbIP15',
     'fd63c1b98f06a07ea95b226664266b277b2b620f1585eb756f4b68fe415a82cc'),
    ('Stylized_Bricks_005_basecolor.png', '1k4pOYacWaR74fjqbEmOPDdXdz_PRidD9',
     'b0b7d98457e42c418099d1eee62a482a9fb423aa20fd831d0d52d24357f0ae29'),
    ('Stylized_Stone_Wall_002_basecolor.png', '1NbT-enoZeAP4pFgdO3gPHV3GdBVJA1Z-',
     '67a57cc6663e87e30e2cc8d1b356471d03cceec7e1f62fe1e91282115ec71471'),
    ('Stylized_Stone_Wall_002_ambientOcclusion.png', '1KflPKIhp1HtzubYMHsAtNBuIDxFgGJ7J',
     '10e7660d3cc25548794db066a9773c82780daa71087df5ef1c2e789faff19c02'),
    ('Stylized_Wood_Planks_002_basecolor.png', '1J2LVyVfXrQ2TNeieBuLIuAER9XPJ73K6',
     '7d8f27be61e6ca045fbf62b858b49560f4cf7c8b7e98d2a2950aca5508e7716e'),
    ('Stylized_Wood_Planks_002_ambientOcclusion.png', '1shluGmzQaUJ3b1pwkdHfXO5z3zYprFLi',
     'a32f6da4d0da4dffe719ffbee7312cee996f2982d5e9230165994d0304a55ce0'),
    ('Stylized_Metal_Plates_001_basecolor.png', '1WjNx-l1Q4zC8rm5oq-M9QPS2Psy6osQb',
     'f578fac122993368e992918cd5fc634658529b65e987e40a52cf9a0da8d6d33f'),
    ('Stylized_Metal_Plates_001_ambientOcclusion.png', '1j5U3HhLneUvUwi3rrmuJ-YaFBkskb6j0',
     '6975cb0f877aec36f9c9be7a9378e32f81220ad3a30b9ae583e937ba53424faa'),
    ('Stylized_Metal_Pattern_001_basecolor.png', '1Ty_p7ArzXsjOol2gihqyNsibe8teTe1j',
     '198ab435c3461fb5cde57500fde627c6a300b3eaf25c5578f458ff57013e40ea'),
    ('Stylized_Metal_Pattern_001_ambientOcclusion.png', '1idZ9-rJOhEXIAF6PJyHRK4_1TyuMU5kr',
     '00617aae6d1bc0b72c1a97d2d1db528afe663d85a408d34a0de60dfb16dffb71'),
    # Map ground options.
    ('Stylized_Sand_002_basecolor.png', '17wH3hq4IOgy3VtlPY41gr_ABiaRcw64w',
     '06a9a0458bb026e52dba31d79a601bfd8d7479b7cdc0b542b492aaac4d6440ae'),
    ('Stylized_Sand_002_ambientOcclusion.png', '1Y3RsL3QWbCf6mNWz-r98VlGaX1NZ8mDy',
     'c47baa68c495e04d539bc2df55e4c4e1384c4c0aa2d60cd1f233c05bbec28db3'),
    ('Snow_005_basecolor.png', '18rntxyJB-drs_86QoYjBtb9-_xKodsXs',
     'c8adeac641e3da4d5d8d1bd29f0372d2a4819e9d4ce9339a1288982a7f321111'),
    ('Snow_005_ambientOcclusion.png', '1j-wGcOMIRz8vyFZ3XLrjSeb7KIQspl_p',
     '013e606df9b980890cb494bface865d07b0eb9262116b4bd61c4c839517ac557'),
    ('Stylized_Grass_003_basecolor.jpg', '1z-JaK_7yyLvU6JfafDOnVpb71kaCXjkx',
     '3a6ad3ce105414d4de9cabaaaef92c038c6a9b48b51880fb6bcfaf54b5ff45e2'),
    ('Stylized_Grass_003_ambientOcclusion.jpg', '1YQ_jzueiZeI-m56zytiuHVCuqbdOTZlw',
     '1385ec1a09f403bed9e248883521f44597bf02a93f9e6a61e2135dc1078867d3'),
)


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def fetch(name, drive_id, digest):
    path = CACHE / name
    if path.is_file() and sha256(path) == digest:
        return
    url = f'https://drive.usercontent.google.com/download?id={drive_id}&export=download&confirm=t'
    request = urllib.request.Request(url, headers={'User-Agent': 'arena-texture-fetch'})
    with urllib.request.urlopen(request, timeout=300) as response:
        data = response.read()
    if hashlib.sha256(data).hexdigest() != digest:
        raise SystemExit(f'{name}: downloaded file does not match the pinned SHA-256')
    path.write_bytes(data)


def main():
    CACHE.mkdir(parents=True, exist_ok=True)
    for source in SOURCES:
        fetch(*source)
        print('verified', source[0])
    subprocess.run(['cargo', 'run', '--locked', '--release', '-p', 'arena-textures', '--',
                    str(CACHE), str(OUTPUT)], check=True, cwd=ROOT)


if __name__ == '__main__':
    sys.exit(main())
