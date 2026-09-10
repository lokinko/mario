export function LoadState({
  loading,
  error,
  onRetry,
}: {
  loading: boolean;
  error: string;
  onRetry: () => void;
}) {
  if (loading) return <p role="status">正在读取本地记录…</p>;
  if (!error) return null;
  return (
    <div className="error-box" role="alert">
      <span>{error}</span>
      <button className="secondary" onClick={onRetry}>
        重新加载
      </button>
    </div>
  );
}
