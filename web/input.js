// Deterministic browser intent state. No DOM, renderer, clocks, or game authority.
const keys = new Set(['KeyW', 'KeyA', 'KeyS', 'KeyD', 'Digit1', 'Tab', 'Escape', 'KeyR', 'Space']);

export class InputState {
  constructor() {
    this.held = new Set();
    this.pointerWasLocked = false;
    this.cancel();
  }

  get dragging() { return this.mouseButtons !== 0; }
  get looking() { return this.dragging && (this.gesture.distance >= 4 || this.gesture.usedChord); }

  keyDown(event, focused = true) {
    if (!focused || !keys.has(event.code) || event.ctrlKey || event.metaKey || event.altKey) return 'ignore';
    if (event.code === 'Tab' && event.shiftKey) return 'leave';
    this.held.add(event.code);
    if (!event.repeat) {
      if (event.code === 'Digit1') this.attack = true;
      if (event.code === 'Tab') this.select = true;
      if (event.code === 'Escape') this.clear = true;
      if (event.code === 'KeyR') this.reset = true;
      if (event.code === 'Space') this.jump = true;
    }
    return 'game';
  }

  keyUp(code) { this.held.delete(code); }
  requestAttack() { this.attack = true; }
  requestReset() { this.cancel(); this.reset = true; }
  wheel(deltaY) { this.zoom += deltaY; }

  mouseDown({ button, buttons, x, y }) {
    if (button !== 0 && button !== 2) return false;
    if (!this.dragging) this.gesture = { x, y, distance: 0, usedChord: false, startedLeft: button === 0 };
    this.mouseButtons = buttons & 3;
    if (this.mouseButtons === 3 && this.gesture) this.gesture.usedChord = true;
    if (this.looking && (this.mouseButtons & 2)) this.steerPending = true;
    return true;
  }

  mouseMove({ buttons, movementX, movementY }) {
    // Ignore pointer-lock recentering and unrelated mouse movement.
    if (!this.dragging || !this.gesture || !(buttons & this.mouseButtons)) return;
    this.gesture.distance += Math.hypot(movementX, movementY);
    if (this.gesture.distance < 4) return;
    this.orbitX += movementX;
    this.orbitY += movementY;
    if (this.mouseButtons & 2) this.steerPending = true;
    else this.freeLookPending = true;
  }

  mouseUp({ button, buttons }) {
    if (!this.dragging || (button !== 0 && button !== 2)) return false;
    this.mouseButtons = buttons & 3;
    if (this.dragging) return true;
    if (button === 0 && this.gesture?.startedLeft && !this.gesture.usedChord && this.gesture.distance < 4) {
      this.click = [this.gesture.x, this.gesture.y];
    }
    if (button === 2 && this.gesture && !this.gesture.startedLeft && !this.gesture.usedChord && this.gesture.distance < 4) {
      this.focusClick = [this.gesture.x, this.gesture.y];
    }
    this.gesture = null;
    return true;
  }

  // Return whether the adapter should exit a late pointer-lock grant.
  pointerLockChanged(locked) {
    const exitLock = locked && !this.dragging;
    if (!locked && this.pointerWasLocked && this.dragging) this.cancel();
    this.pointerWasLocked = locked;
    return exitLock;
  }

  cancel() {
    this.held.clear();
    this.mouseButtons = 0;
    this.gesture = null;
    this.attack = this.select = this.clear = this.reset = false;
    this.selectPlayer = false;
    this.jump = false;
    this.orbitX = this.orbitY = this.zoom = 0;
    this.steerPending = this.freeLookPending = false;
    this.click = null;
    this.focusClick = null;
    this.selectActor = -1;
    this.teamSize = 0;
  }

  readInput() {
    // Positional ABI consumed by arena-client's platform bridge. Held input
    // persists; edge-triggered actions and accumulated deltas drain once.
    const input = [Number(this.held.has('KeyD')) - Number(this.held.has('KeyA')), this.mouseButtons === 3 ? -1 : Number(this.held.has('KeyS')) - Number(this.held.has('KeyW')), Number(this.attack), Number(this.select), Number(this.clear), Number(this.reset), this.orbitX, this.orbitY, this.zoom, this.click?.[0] ?? -1, this.click?.[1] ?? -1, Number((this.looking && Boolean(this.mouseButtons & 2)) || this.steerPending), Number((this.looking && this.mouseButtons === 1) || this.freeLookPending)];
    input.push(Number(this.selectPlayer));
    input.push(Number(this.jump));
    input.push(this.focusClick?.[0] ?? -1, this.focusClick?.[1] ?? -1);
    input.push(this.selectActor, this.teamSize);
    this.selectActor = -1;
    this.teamSize = 0;
    this.focusClick = null;
    this.jump = false;
    this.selectPlayer = false;
    this.attack = this.select = this.clear = this.reset = false;
    this.orbitX = this.orbitY = this.zoom = 0;
    this.click = null;
    this.steerPending = this.freeLookPending = false;
    return input;
  }
}
