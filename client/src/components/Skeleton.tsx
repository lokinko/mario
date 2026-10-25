export function Skeleton({ rows = 4 }: { rows?: number }) {
  return (
    <div className="skeleton-list" role="status" aria-label="正在加载">
      <span className="sr-only">正在加载…</span>
      {Array.from({ length: rows }, (_, i) => (
        <div key={i} className="skeleton" />
      ))}
    </div>
  );
}
export function ApplicationSkeleton() {
  return (
    <div className="app-skeleton" role="status" aria-label="正在加载投资档案">
      <aside className="skeleton" />
      <div>
        <div className="skeleton skeleton-heading" />
        <Skeleton rows={3} />
      </div>
    </div>
  );
}
