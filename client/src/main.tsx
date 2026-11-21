import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { Website } from "./app/Website";
import { InterfaceProvider } from "./components/InterfaceProvider";
import "./styles/tokens.css";
import "./styles.css";
import "./styles/workspace.css";

try {
  const saved = JSON.parse(localStorage.getItem("mario.display") ?? "{}");
  document.documentElement.dataset.pnl = saved.pnl === "intl" ? "intl" : "cn";
  document.documentElement.dataset.theme =
    saved.theme === "light" || saved.theme === "dark" ? saved.theme : "system";
  document.documentElement.toggleAttribute(
    "data-mask-amounts",
    saved.maskAmounts === true,
  );
} catch {
  document.documentElement.dataset.pnl = "cn";
  document.documentElement.dataset.theme = "system";
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <InterfaceProvider>
      <Website />
    </InterfaceProvider>
  </StrictMode>,
);
