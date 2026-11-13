import { lazy } from "react";
import App from "../App";
import { DeferredPage } from "../components/DeferredPage";

const Dashboard = lazy(() =>
  import("../features/dashboard/UsageDashboard").then((m) => ({
    default: m.UsageDashboard,
  })),
);

export function Website() {
  const path = window.location.pathname.replace(/\/+$/, "");
  return path === `${import.meta.env.BASE_URL}dashboard` ? (
    <DeferredPage>
      <Dashboard />
    </DeferredPage>
  ) : (
    <App />
  );
}
