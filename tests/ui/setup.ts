// jsdom has no layout engine. Supply only the observer API needed by Radix;
// these tests make no assertions about rendered geometry.
globalThis.ResizeObserver = class {
  observe() {}
  unobserve() {}
  disconnect() {}
};

// jsdom does not implement pointer capture or PointerEvent; exercise explicit
// event transitions without pretending this is native pointer-lock/drag review.
class TestPointerEvent extends MouseEvent {
  readonly pointerId: number;
  constructor(type: string, init: PointerEventInit = {}) {
    super(type, init);
    this.pointerId = init.pointerId ?? 1;
  }
}
Object.defineProperty(globalThis, "PointerEvent", { configurable: true, value: TestPointerEvent });
Element.prototype.setPointerCapture = () => {};
Element.prototype.releasePointerCapture = () => {};
