import { ArrowLeftRight, X } from "lucide-react";
import { useEffect, useId, useRef, useState } from "react";
import { ExchangeRates } from "./ExchangeRates";

export function FloatingExchangeRates(
  props: Parameters<typeof ExchangeRates>[0],
) {
  const [open, setOpen] = useState(false);
  const [visited, setVisited] = useState(false);
  const panelId = useId();
  const trigger = useRef<HTMLButtonElement>(null);
  const close = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (!open) return;
    close.current?.focus();
    const keyboard = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !document.querySelector(".mobile-open")) {
        setOpen(false);
        trigger.current?.focus();
      }
    };
    window.addEventListener("keydown", keyboard);
    return () => window.removeEventListener("keydown", keyboard);
  }, [open]);

  return (
    <aside className="exchange-float" aria-label="汇率浮窗">
      {visited && (
        <div className="exchange-float-panel" id={panelId} hidden={!open}>
          <div className="exchange-float-bar">
            <span>
              <ArrowLeftRight size={16} /> 汇率
            </span>
            <button
              ref={close}
              className="text-button"
              aria-label="收起汇率"
              onClick={() => {
                setOpen(false);
                trigger.current?.focus();
              }}
            >
              <X size={18} />
            </button>
          </div>
          <div className="exchange-float-content">
            <ExchangeRates {...props} />
          </div>
        </div>
      )}
      <button
        ref={trigger}
        className="exchange-float-trigger"
        aria-label={open ? "收起汇率浮窗" : "展开汇率浮窗"}
        aria-expanded={open}
        aria-controls={visited ? panelId : undefined}
        onClick={() => {
          setVisited(true);
          setOpen((current) => !current);
        }}
      >
        <ArrowLeftRight size={18} />
        <span>汇率</span>
      </button>
    </aside>
  );
}
