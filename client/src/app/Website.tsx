import { lazy } from "react";
import { DeferredPage } from "../components/DeferredPage";
import { WebAccess } from "../features/account/WebAccess";
import {
  dividendPreviewHref,
  isDividendPreview,
} from "../features/watch/dividend-preview/route";

const App = lazy(() => import("../App"));

const DividendPreview = lazy(() =>
  import("../features/watch/dividend-preview/DividendPreview").then((m) => ({
    default: m.DividendPreview,
  })),
);

const Dashboard = lazy(() =>
  import("../features/dashboard/UsageDashboard").then((m) => ({
    default: m.UsageDashboard,
  })),
);

export function Website() {
  const path = window.location.pathname.replace(/\/+$/, "");
  if (isDividendPreview(window.location))
    return (
      <DeferredPage>
        <DividendPreview />
      </DeferredPage>
    );
  return (
    <WebAccess publicPreviewHref={dividendPreviewHref()}>
      {path === `${import.meta.env.BASE_URL}dashboard` ? (
        <DeferredPage>
          <Dashboard />
        </DeferredPage>
      ) : (
        <DeferredPage>
          <App />
        </DeferredPage>
      )}
    </WebAccess>
  );
}
