import { lazy } from "react";
import { DeferredPage } from "../components/DeferredPage";
import { WebAccess } from "../features/account/WebAccess";
import {
  dividendPreviewHref,
  isDividendPreview,
} from "../features/watch/dividend-preview/route";
import { preloadInitialRoute } from "./preload";

let appPromise: Promise<typeof import("../App")> | null = null;
const loadApp = () => (appPromise ??= import("../App"));
const preloadWorkspace = () => {
  void loadApp();
  preloadInitialRoute();
};

const App = lazy(loadApp);

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
    <WebAccess
      publicPreviewHref={dividendPreviewHref()}
      onPreload={preloadWorkspace}
    >
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
