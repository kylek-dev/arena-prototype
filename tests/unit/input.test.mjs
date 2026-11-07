import test from 'node:test';
import assert from 'node:assert/strict';
import { InputState } from '../../web/input.js';

const sample = (input) => {
  const [x, z, attack, select, clear, reset, orbitX, orbitY, zoom, clickX, clickY, steer, freeLook] = input.readInput();
  return { x, z, attack, select, clear, reset, orbitX, orbitY, zoom, clickX, clickY, steer, freeLook };
};
const down = (input, button, buttons) => input.mouseDown({ button, buttons, x: 25, y: 40 });
const move = (input, buttons, movementX = 12, movementY = 5) => input.mouseMove({ buttons, movementX, movementY });

const idle = sample(new InputState());

test('Space appends a one-shot jump without changing existing ABI fields', () => {
  const input = new InputState();
  const before = input.readInput();
  input.keyDown({ code: 'Space' });
  const jumped = input.readInput();
  assert.deepEqual(jumped.slice(0, 14), before.slice(0, 14));
  assert.equal(jumped[14], 1);
  assert.equal(input.readInput()[14], 0);
  input.keyDown({ code: 'Space', repeat: true });
  assert.equal(input.readInput()[14], 0);
  input.keyUp('Space');
  input.keyDown({ code: 'Space' });
  assert.equal(input.readInput()[14], 1);
  input.keyDown({ code: 'Space' });
  input.cancel();
  assert.equal(input.readInput()[14], 0);
});

test('keyboard movement persists between samples and opposing keys cancel', () => {
  const input = new InputState();
  input.keyDown({ code: 'KeyW' });
  input.keyDown({ code: 'KeyD' });
  assert.equal(sample(input).z, -1);
  assert.equal(sample(input).x, 1);
  input.keyDown({ code: 'KeyS' });
  input.keyDown({ code: 'KeyA' });
  assert.deepEqual(sample(input), idle);
  input.keyUp('KeyD');
  input.keyUp('KeyW');
  assert.equal(sample(input).x, -1);
  assert.equal(sample(input).z, 1);
});

for (const [code, action] of [['Digit1', 'attack'], ['Tab', 'select'], ['Escape', 'clear'], ['KeyR', 'reset']]) {
  test(`${code} emits ${action} once, ignores repeats, and rearms on a new press`, () => {
    const input = new InputState();
    input.keyDown({ code });
    assert.equal(sample(input)[action], 1);
    assert.equal(sample(input)[action], 0);
    input.keyDown({ code, repeat: true });
    assert.equal(sample(input)[action], 0);
    input.keyUp(code);
    input.keyDown({ code });
    assert.equal(sample(input)[action], 1);
  });
}

test('unfocused input, unrelated keys, and browser shortcuts leave intent unchanged', () => {
  const input = new InputState();
  assert.equal(input.keyDown({ code: 'KeyW' }, false), 'ignore');
  assert.equal(input.keyDown({ code: 'F5' }), 'ignore');
  for (const modifier of ['ctrlKey', 'metaKey', 'altKey']) {
    assert.equal(input.keyDown({ code: 'KeyR', [modifier]: true }), 'ignore');
  }
  assert.equal(input.keyDown({ code: 'Tab', shiftKey: true }), 'leave');
  assert.deepEqual(sample(input), idle);
});

test('left drag accumulates free look without mouse-driven walking or steering', () => {
  const input = new InputState();
  down(input, 0, 1);
  move(input, 1);
  move(input, 1, 3, -2);
  assert.deepEqual(sample(input), { ...idle, orbitX: 15, orbitY: 3, freeLook: 1 });
  assert.deepEqual(sample(input), { ...idle, freeLook: 1 });
  input.mouseUp({ button: 0, buttons: 0 });
  assert.deepEqual(sample(input), idle);
});

test('right drag steers without walking and retains held keyboard movement on release', () => {
  const input = new InputState();
  down(input, 2, 2);
  move(input, 2);
  assert.deepEqual(sample(input), { ...idle, orbitX: 12, orbitY: 5, steer: 1 });
  input.keyDown({ code: 'KeyW' });
  input.mouseUp({ button: 2, buttons: 0 });
  assert.deepEqual(sample(input), { ...idle, z: -1 });
});

for (const [first, second] of [[0, 2], [2, 0]]) {
  test(`buttons ${first} then ${second} walk and steer; partial release retains only the remaining mode`, () => {
    const input = new InputState();
    down(input, first, first === 0 ? 1 : 2);
    down(input, second, 3);
    move(input, 3);
    assert.deepEqual(sample(input), { ...idle, z: -1, orbitX: 12, orbitY: 5, steer: 1 });
    assert.deepEqual(sample(input), { ...idle, z: -1, steer: 1 });
    const remaining = second === 0 ? 1 : 2;
    input.mouseUp({ button: first, buttons: remaining });
    assert.equal(input.dragging, true);
    const mode = second === 0 ? 'freeLook' : 'steer';
    assert.deepEqual(sample(input), { ...idle, [mode]: 1 });
    move(input, remaining);
    assert.deepEqual(sample(input), { ...idle, orbitX: 12, orbitY: 5, [mode]: 1 });
    input.mouseUp({ button: second, buttons: 0 });
    assert.equal(input.dragging, false);
    assert.deepEqual(sample(input), idle); // A chord must never select a target.
  });
}

test('both-button walking overrides backward keyboard input until partial release', () => {
  const input = new InputState();
  input.keyDown({ code: 'KeyS' });
  down(input, 0, 1);
  down(input, 2, 3);
  assert.equal(sample(input).z, -1);
  input.mouseUp({ button: 0, buttons: 2 });
  assert.equal(sample(input).z, 1);
});

test('an ordinary left click emits canvas-relative coordinates once', () => {
  const input = new InputState();
  down(input, 0, 1);
  move(input, 1, 2, 0);
  input.mouseUp({ button: 0, buttons: 0 });
  assert.deepEqual(sample(input), { ...idle, clickX: 25, clickY: 40 });
  assert.deepEqual(sample(input), idle);
});

for (const distance of [4, 5]) {
  test(`movement of ${distance}px is a drag and cannot become a target click`, () => {
    const input = new InputState();
    down(input, 0, 1);
    move(input, 1, distance, 0);
    input.mouseUp({ button: 0, buttons: 0 });
    assert.deepEqual(sample(input), { ...idle, orbitX: distance, freeLook: 1 });
    assert.deepEqual(sample(input), idle);
  });
}

test('drag threshold uses cumulative path length even when motion reverses', () => {
  const input = new InputState();
  down(input, 0, 1);
  move(input, 1, 2, 0);
  move(input, 1, -2, 0);
  input.mouseUp({ button: 0, buttons: 0 });
  assert.equal(sample(input).clickX, -1);
});

test('right clicks, middle buttons, and idle releases cannot select a target', () => {
  const input = new InputState();
  assert.equal(down(input, 1, 4), false);
  assert.equal(input.dragging, false);
  assert.equal(input.mouseUp({ button: 0, buttons: 0 }), false);
  down(input, 2, 2);
  input.mouseUp({ button: 1, buttons: 2 });
  assert.equal(input.dragging, true);
  input.mouseUp({ button: 2, buttons: 0 });
  assert.deepEqual(sample(input), idle);
});

test('zero-button recentering and unrelated motion do not change look intent', () => {
  const input = new InputState();
  move(input, 1);
  down(input, 0, 1);
  move(input, 0, 2000, 2000);
  move(input, 2, 2000, 2000);
  assert.deepEqual(sample(input), idle);
});

test('a short drag drained after release retains its last steering intent for one sample', () => {
  const input = new InputState();
  down(input, 2, 2);
  move(input, 2);
  input.mouseUp({ button: 2, buttons: 0 });
  assert.deepEqual(sample(input), { ...idle, orbitX: 12, orbitY: 5, steer: 1 });
  assert.deepEqual(sample(input), idle);
});

function dirtyInput() {
  const input = new InputState();
  input.keyDown({ code: 'KeyW' });
  input.keyDown({ code: 'Tab' });
  input.keyDown({ code: 'Escape' });
  input.keyDown({ code: 'KeyR' });
  input.requestAttack();
  input.wheel(50);
  down(input, 0, 1);
  input.mouseUp({ button: 0, buttons: 0 }); // Queued click.
  down(input, 2, 2);
  down(input, 0, 3);
  move(input, 3);
  return input;
}

test('focus/visibility cancellation clears held keys, capture, queued actions, clicks and deltas', () => {
  const input = dirtyInput();
  input.cancel();
  assert.equal(input.dragging, false);
  assert.deepEqual(sample(input), idle);
  input.mouseUp({ button: 0, buttons: 0 });
  assert.deepEqual(sample(input), idle);
});

test('pointer-lock loss cancels all input; duplicate unlocked notifications do not cancel fresh input', () => {
  const input = dirtyInput();
  assert.equal(input.pointerLockChanged(true), false);
  assert.equal(input.pointerLockChanged(false), false);
  assert.deepEqual(sample(input), idle);
  input.keyDown({ code: 'KeyW' });
  input.pointerLockChanged(false);
  assert.equal(sample(input).z, -1);
});

test('a late async pointer-lock grant requests exit without resurrecting mouse input', () => {
  const input = new InputState();
  down(input, 2, 2);
  input.mouseUp({ button: 2, buttons: 0 });
  sample(input);
  assert.equal(input.pointerLockChanged(true), true);
  input.pointerLockChanged(false);
  assert.deepEqual(sample(input), idle);
});

test('lock denial retains drag fallback and keyboard input', () => {
  const input = new InputState();
  input.keyDown({ code: 'KeyW' });
  down(input, 2, 2);
  input.pointerLockChanged(false);
  move(input, 2);
  assert.deepEqual(sample(input), { ...idle, z: -1, orbitX: 12, orbitY: 5, steer: 1 });
});

test('HUD reset discards pending input and emits reset once', () => {
  const input = dirtyInput();
  input.requestReset();
  assert.deepEqual(sample(input), { ...idle, reset: 1 });
  assert.deepEqual(sample(input), idle);
  input.requestAttack();
  assert.equal(sample(input).attack, 1);
  assert.deepEqual(sample(input), idle);
});

test('the Rust bridge input array keeps its positional contract and drains deltas', () => {
  const input = new InputState();
  input.keyDown({ code: 'KeyW' });
  input.keyDown({ code: 'Tab' });
  input.requestAttack();
  input.wheel(150);
  input.wheel(-30);
  down(input, 2, 2);
  move(input, 2, 10, 5);
  assert.deepEqual(input.readInput(), [0, -1, 1, 1, 0, 0, 10, 5, 120, -1, -1, 1, 0, 0, 0]);
  assert.deepEqual(input.readInput(), [0, -1, 0, 0, 0, 0, 0, 0, 0, -1, -1, 1, 0, 0, 0]);
});

test('self selection uses the appended bridge field and drains or cancels once', () => {
  const input = new InputState();
  input.selectPlayer = true;
  assert.equal(input.readInput()[13], 1);
  assert.equal(input.readInput()[13], 0);
  input.selectPlayer = true;
  input.cancel();
  assert.equal(input.readInput()[13], 0);
});
