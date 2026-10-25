import type { ReactNode } from "react";
import { localDateValue } from "../lib/dates";
export function LedgerEntry({
  date,
  children,
  inactive = false,
  showMonth = false,
}: {
  date: string;
  children: ReactNode;
  inactive?: boolean;
  showMonth?: boolean;
}) {
  const day = localDateValue(new Date(date));
  return (
    <>
      {showMonth && (
        <h3 className="month-divider">
          {day.slice(0, 4)} 年 {day.slice(5, 7)} 月
        </h3>
      )}
      <article className={`ledger-entry${inactive ? " inactive" : ""}`}>
        <time className="timeline-date" dateTime={date}>
          <strong>{day.slice(5)}</strong>
          <span>{day.slice(0, 4)}</span>
        </time>
        <div>{children}</div>
      </article>
    </>
  );
}
