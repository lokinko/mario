import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { WebAccess } from "./features/account/WebAccess";
import "./styles.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <WebAccess>
      <App />
    </WebAccess>
  </StrictMode>,
);
