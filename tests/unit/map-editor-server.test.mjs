import test from 'node:test';
import assert from 'node:assert/strict';
import http from 'node:http';
import { mkdtemp, mkdir, readFile, readdir, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { mapEditorMiddleware } from '../../scripts/map-editor-server.mjs';

const map = { format: 1, name: 'Test', floor: { width: 40, depth: 30 } };

async function serve(t) {
  const root = await mkdtemp(path.join(os.tmpdir(), 'map-editor-'));
  await mkdir(path.join(root, 'assets', 'maps'), { recursive: true });
  await writeFile(path.join(root, 'assets', 'maps', 'colosseum.json'), '{"format":1}\n');
  const middleware = mapEditorMiddleware(root);
  const server = http.createServer((req, res) =>
    middleware(req, res, () => {
      res.statusCode = 404;
      res.end('next');
    }),
  );
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  t.after(async () => {
    server.close();
    await rm(root, { recursive: true, force: true });
  });
  const base = `http://127.0.0.1:${server.address().port}`;
  // Raw requests so tests control Host and Origin, which fetch forbids.
  const request = (method, url, { body, headers = {} } = {}) =>
    new Promise((resolve, reject) => {
      const req = http.request(`${base}${url}`, { method, headers }, (res) => {
        let text = '';
        res.on('data', (c) => (text += c));
        res.on('end', () => resolve({ status: res.statusCode, text }));
      });
      req.on('error', reject);
      req.end(body);
    });
  return { root, request, host: `127.0.0.1:${server.address().port}` };
}

const json = { 'Content-Type': 'application/json' };

test('serves the tracked map fresh and reports that saving is available', async (t) => {
  const { request } = await serve(t);
  assert.deepEqual(await request('GET', '/assets/maps/colosseum.json?x=1'), { status: 200, text: '{"format":1}\n' });
  assert.equal((await request('GET', '/assets/maps/missing.json')).text, 'next');
  assert.equal((await request('GET', '/assets/maps/..%2Fsecret.json')).text, 'next');
  assert.deepEqual(JSON.parse((await request('GET', '/__map-editor/status')).text), { writable: true });
  assert.deepEqual(JSON.parse((await request('GET', '/__map-editor/maps')).text), {
    maps: [{ id: 'colosseum', name: 'colosseum' }],
  });
  assert.equal((await request('GET', '/other')).text, 'next');
});

test('saves a same-origin JSON map to assets/maps with a trailing newline', async (t) => {
  const { root, request, host } = await serve(t);
  const saved = await request('PUT', '/__map-editor/maps/colosseum.json', {
    body: JSON.stringify(map),
    headers: { ...json, Origin: `http://${host}` },
  });
  assert.equal(saved.status, 200, saved.text);
  assert.deepEqual(JSON.parse(saved.text), { saved: 'assets/maps/colosseum.json' });
  const text = await readFile(path.join(root, 'assets', 'maps', 'colosseum.json'), 'utf8');
  assert.equal(text, `${JSON.stringify(map)}\n`);
  assert.deepEqual(await readdir(path.join(root, 'assets', 'maps')), ['colosseum.json']);
});

test('refuses cross-origin, rebinding, non-JSON, oversized, malformed and unsafe saves', async (t) => {
  const { root, request, host } = await serve(t);
  const put = (url, body, headers) => request('PUT', url, { body, headers });
  const body = JSON.stringify(map);
  const cases = [
    [put('/__map-editor/maps/colosseum.json', body, { ...json, Origin: 'http://evil.test' }), 403],
    [put('/__map-editor/maps/colosseum.json', body, { ...json, Host: 'evil.test:8000' }), 403],
    [put('/__map-editor/maps/colosseum.json', body, { 'Content-Type': 'text/plain' }), 415],
    [put('/__map-editor/maps/colosseum.json', 'x'.repeat(300 * 1024), json), 413],
    [put('/__map-editor/maps/colosseum.json', '{', json), 400],
    [put('/__map-editor/maps/colosseum.json', '{"format":2}', json), 400],
    [put('/__map-editor/maps/Bad_Name.json', body, json), 400],
    [request('POST', '/__map-editor/maps/colosseum.json', { body, headers: json }), 405],
  ];
  for (const [response, status] of cases) {
    const { status: actual, text } = await response;
    assert.equal(actual, status, text);
  }
  assert.equal(await readFile(path.join(root, 'assets', 'maps', 'colosseum.json'), 'utf8'), '{"format":1}\n');
  assert.ok(host);
});

test('lists map files by id with their names, skipping other files', async (t) => {
  const { root, request } = await serve(t);
  const dir = path.join(root, 'assets', 'maps');
  await writeFile(path.join(dir, 'frozen-pit.json'), JSON.stringify({ format: 1, name: 'Frozen Pit' }));
  await writeFile(path.join(dir, 'Bad Name.json'), '{}');
  await writeFile(path.join(dir, 'notes.txt'), 'x');
  const listed = JSON.parse((await request('GET', '/__map-editor/maps')).text);
  assert.deepEqual(listed.maps, [
    { id: 'colosseum', name: 'colosseum' },
    { id: 'frozen-pit', name: 'Frozen Pit' },
  ]);
});
