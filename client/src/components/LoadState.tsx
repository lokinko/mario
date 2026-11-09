import { Skeleton } from "./Skeleton";
import { Callout } from "./Callout";
import { Button } from "antd";
export function LoadState({
  loading,
  error,
  onRetry,
}: {
  loading: boolean;
  error: string;
  onRetry: () => void;
}) {
  if (loading) return <Skeleton />;
  if (!error) return null;
  return (
    <Callout tone="danger">
      <span>{error}</span>
      <Button onClick={onRetry}>重新加载</Button>
    </Callout>
  );
}
