#!/usr/bin/env python3
"""Check complete-build files and HTTP MIME/path wiring, not a rendered browser."""

from functools import partial
from html.parser import HTMLParser
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from threading import Thread
from urllib.request import Request, urlopen
import json


ROOT = Path(__file__).resolve().parent.parent
DIST = ROOT / 'dist'


class References(HTMLParser):
    def __init__(self):
        super().__init__()
        self.urls = []

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        key = 'src' if tag == 'script' else 'href' if tag == 'link' else None
        value = attrs.get(key, '')
        if value and not value.startswith('data:'):
            assert value.startswith('./'), f'Non-relative build URL: {value}'
            self.urls.append(value)


class QuietHandler(SimpleHTTPRequestHandler):
    def log_message(self, *_args):
        pass


parser = References()
parser.feed((DIST / 'index.html').read_text())
assert parser.urls, 'No built frontend references'
manifest = json.loads((DIST / 'assets/character.json').read_text())
if manifest['available']:
    assert (DIST / 'assets/characters/pixellabs-undead-3417.glb').is_file()

paths = ['index.html', 'pkg/arena_client.js', 'pkg/arena_client_bg.wasm',
         'assets/character.json', *parser.urls]
server = ThreadingHTTPServer(('127.0.0.1', 0), partial(QuietHandler, directory=str(ROOT)))
thread = Thread(target=server.serve_forever, daemon=True)
thread.start()
try:
    for path in paths:
        # Serving ROOT makes /dist/ a real non-root hosting prefix.
        url = f'http://127.0.0.1:{server.server_port}/dist/{path}'
        with urlopen(Request(url, method='HEAD')) as response:
            assert response.status == 200, path
            assert int(response.headers['Content-Length']) > 0, path
            if path.endswith('.wasm'):
                assert response.headers.get_content_type() == 'application/wasm'
    print(f'PASS: {len(paths)} static resources under /dist/; WASM MIME correct. No browser used.')
finally:
    server.shutdown()
    server.server_close()
    thread.join()
