import { Component, Suspense, type ReactNode } from "react";
import { Button } from "antd";
import { Callout } from "./Callout";
import { Skeleton } from "./Skeleton";

class PageErrorBoundary extends Component<
  { children: ReactNode; onReload: () => void },
  { failed: boolean }
> {
  state = { failed: false };
  static getDerivedStateFromError() {
    return { failed: true };
  }
  render() {
    if (!this.state.failed) return this.props.children;
    return (
      <Callout
        tone="danger"
        action={
          <Button
            onClick={() => {
              if (window.confirm("重新加载会清除尚未保存的输入，确认继续？"))
                this.props.onReload();
            }}
          >
            重新加载
          </Button>
        }
      >
        页面暂时无法打开，请重新加载后重试。
      </Callout>
    );
  }
}

/** Each page owns its loading/error boundary; a failed sibling never unmounts the conversation. */
export function DeferredPage({
  children,
  onReload = () => window.location.reload(),
}: {
  children: ReactNode;
  onReload?: () => void;
}) {
  return (
    <PageErrorBoundary onReload={onReload}>
      <Suspense fallback={<Skeleton />}>{children}</Suspense>
    </PageErrorBoundary>
  );
}
