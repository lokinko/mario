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
}
