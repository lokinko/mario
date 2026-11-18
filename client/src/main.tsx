import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { Website } from "./app/Website";
import { InterfaceProvider } from "./components/InterfaceProvider";
import "./styles/tokens.css";
import "./styles.css";
import "./styles/workspace.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <InterfaceProvider>
      <Website />
    </InterfaceProvider>
  </StrictMode>,
);
