import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { Website } from "./app/Website";
import { WebAccess } from "./features/account/WebAccess";
import { InterfaceProvider } from "./components/InterfaceProvider";
import "./styles/tokens.css";
import "./styles.css";
import "./styles/workspace.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <InterfaceProvider>
      <WebAccess>
        <Website />
      </WebAccess>
    </InterfaceProvider>
  </StrictMode>,
);
