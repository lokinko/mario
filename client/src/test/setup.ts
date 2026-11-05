// Node 25+ defines a global Web Storage that is undefined without --localstorage-file
// and shadows the jsdom implementation; jsdom tests must use the window's storage.
const dom = (globalThis as { jsdom?: { window: Window } }).jsdom;
if (dom) {
  for (const name of ["localStorage", "sessionStorage"] as const) {
    Object.defineProperty(globalThis, name, {
      configurable: true,
      value: dom.window[name],
    });
  }
  // jsdom has no layout observer. Responsive layout is checked in the browser;
  // Ant Design's textarea still needs the observer interface to mount in unit tests.
  Object.defineProperty(globalThis, "ResizeObserver", {
    configurable: true,
    value: class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  });
}
