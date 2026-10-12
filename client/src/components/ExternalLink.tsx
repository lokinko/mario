import type { AnchorHTMLAttributes } from "react";

export function ExternalLink({
  href,
  children,
  ...props
}: AnchorHTMLAttributes<HTMLAnchorElement>) {
  let safeUrl: string | undefined;
  try {
    const url = new URL(href || "");
    if (["https:", "http:"].includes(url.protocol)) safeUrl = url.href;
  } catch {
    // Sources without a usable URL remain readable as text.
  }
  if (!safeUrl) return <span>{children}</span>;
  return (
    <a target="_blank" rel="noreferrer" {...props} href={safeUrl}>
      {children}
    </a>
  );
}
