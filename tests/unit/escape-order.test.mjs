import test from 'node:test';
import assert from 'node:assert/strict';
import { escapeStopsCast } from '../../web/src/domain/bindings/escape.ts';
import { InputState } from '../../web/input.js';

const idle = { capturing: false, textEntry: false, casting: false };

test('Escape stops the current cast first', () => {
  assert.equal(escapeStopsCast({ ...idle, casting: true }), true);
});

test('without a cast, Escape is left to the panel and menu handlers', () => {
  assert.equal(escapeStopsCast(idle), false);
});

test('binding capture and text fields keep Escape even while casting', () => {
  assert.equal(escapeStopsCast({ ...idle, capturing: true, casting: true }), false);
  assert.equal(escapeStopsCast({ ...idle, textEntry: true, casting: true }), false);
});

test('a stop-casting request is ABI field 21, drains once and is cleared by cancel', () => {
  const input = new InputState();
  const before = input.readInput();
  assert.equal(before.length, 22);
  assert.equal(before[21], 0);
  input.requestStopCast();
  const stopped = input.readInput();
  assert.deepEqual(stopped.slice(0, 21), before.slice(0, 21));
  assert.equal(stopped[21], 1);
  assert.equal(input.readInput()[21], 0);
  input.requestStopCast();
  input.cancel();
  assert.equal(input.readInput()[21], 0);
});
