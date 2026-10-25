import { Skeleton } from "./Skeleton";
import { Callout } from "./Callout";
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
      <button className="secondary" onClick={onRetry}>
        重新加载
      </button>
    </Callout>
  );
}
