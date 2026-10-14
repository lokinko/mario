import { createContext, useContext, type ReactNode } from "react";
export const PageStatus = createContext<ReactNode>(null);
export function PageHeader({
  title,
  description,
  action,
}: {
  title: string;
  description?: string;
  action?: ReactNode;
}) {
  const status = useContext(PageStatus);
  return (
    <header className="page-header">
      <div>
        <h1 tabIndex={-1}>{title}</h1>
        {description && <p>{description}</p>}
      </div>
      <div className="page-actions">
        {status}
        {action}
      </div>
    </header>
  );
}
