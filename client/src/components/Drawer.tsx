import {
  createContext,
  useContext,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";
import { X } from "lucide-react";

const DrawerControls = createContext({
  markClean: () => {},
  setBusy: (_busy: boolean) => {},
  confirmDiscard: (): boolean => true,
});
export const useDrawerControls = () => useContext(DrawerControls);
/** One modal surface shared by detail and editor modes. */
export function Drawer({
  title,
  children,
  onClose,
  busy = false,
  dirty = false,
  resetKey,
}: {
  title: string;
  children: ReactNode;
  onClose: () => void;
  busy?: boolean;
  dirty?: boolean;
  resetKey?: unknown;
}) {
  const [nestedBusy, setNestedBusy] = useState(false);
  const heading = useId();
  const panel = useRef<HTMLDivElement>(null);
  // Capture before child autoFocus runs during commit.
  const returnFocus = useRef(document.activeElement as HTMLElement | null);
  const changed = useRef(false);
  const options = useRef({ onClose, busy, dirty });
  options.current = { onClose, busy: busy || nestedBusy, dirty };
  const controls = useMemo(
    () => ({
      markClean: () => {
        changed.current = false;
      },
      setBusy: setNestedBusy,
      confirmDiscard: () =>
        !(changed.current || options.current.dirty) ||
        window.confirm("有未保存的修改，确认放弃？"),
    }),
    [],
  );
  useEffect(() => {
    changed.current = false;
  }, [resetKey]);
  const dismiss = () => {
    const current = options.current;
    if (current.busy) return;
    if (
      (changed.current || current.dirty) &&
      !window.confirm("有未保存的修改，确认放弃？")
    )
      return;
    current.onClose();
  };
  const close = useRef(dismiss);
  close.current = dismiss;
  useEffect(() => {
    const previous = returnFocus.current;
    const overflow = document.body.style.overflow;
    const siblings = Array.from(document.body.children).filter(
      (node): node is HTMLElement =>
        node instanceof HTMLElement && node !== panel.current?.parentElement,
    );
    const states = siblings.map((node) => node.inert);
    siblings.forEach((node) => {
      node.inert = true;
    });
    document.body.style.overflow = "hidden";
    document.body.dataset.drawerOpen = "true";
    panel.current?.querySelector<HTMLElement>("button")?.focus();
    const keyboard = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopImmediatePropagation();
        close.current();
      }
      if (event.key !== "Tab") return;
      const controls = Array.from(
        panel.current?.querySelectorAll<HTMLElement>(
          'button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), a[href], summary, [tabindex="0"]',
        ) ?? [],
      ).filter(
        (node) => !node.closest("[hidden]") && node.getClientRects().length > 0,
      );
      const first = controls[0],
        last = controls.at(-1);
      if (!first) {
        event.preventDefault();
        panel.current?.focus();
      } else if (
        event.shiftKey &&
        (document.activeElement === first ||
          document.activeElement === panel.current)
      ) {
        event.preventDefault();
        last?.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    window.addEventListener("keydown", keyboard, true);
    return () => {
      window.removeEventListener("keydown", keyboard, true);
      siblings.forEach((node, index) => {
        node.inert = states[index];
      });
      document.body.style.overflow = overflow;
      delete document.body.dataset.drawerOpen;
      if (previous?.isConnected) previous.focus({ preventScroll: true });
      else
        document
          .querySelector<HTMLElement>(".page-header h1")
          ?.focus({ preventScroll: true });
    };
  }, []);
  return createPortal(
    <div className="drawer-layer">
      <div className="drawer-backdrop" onClick={dismiss} />
      <div
        ref={panel}
        className="drawer"
        role="dialog"
        aria-modal="true"
        aria-labelledby={heading}
        tabIndex={-1}
        onChangeCapture={(event) => {
          if (!(event.target as HTMLElement).closest("[data-presentation]"))
            changed.current = true;
        }}
      >
        <header className="drawer-header">
          <h2 id={heading}>{title}</h2>
          <button
            type="button"
            className="icon-button"
            aria-label="关闭详情"
            disabled={busy || nestedBusy}
            onClick={dismiss}
          >
            <X size={20} />
          </button>
        </header>
        <DrawerControls.Provider value={controls}>
          <div className="drawer-body">{children}</div>
        </DrawerControls.Provider>
      </div>
    </div>,
    document.body,
  );
}
