import { useState, type AnchorHTMLAttributes } from "react";
import { isNative } from "../lib/service";

export function ExternalLink({
  href,
  children,
  onClick,
  ...props
}: AnchorHTMLAttributes<HTMLAnchorElement>) {
  const [error, setError] = useState("");
  let safeUrl: string | undefined;
  try {
    const url = new URL(href || "");
    if (["https:", "http:"].includes(url.protocol)) safeUrl = url.href;
  } catch {
    // Sources without a usable URL remain readable as text.
  }
  if (!safeUrl) return <span>{children}</span>;
  return (
    <>
      <a
        target="_blank"
        rel="noreferrer"
        {...props}
        href={safeUrl}
        onClick={(event) => {
          onClick?.(event);
          if (event.defaultPrevented || !isNative()) return;
          event.preventDefault();
          setError("");
          void import("@tauri-apps/plugin-opener")
            .then(({ openUrl }) => openUrl(safeUrl!))
            .catch((nextError) =>
              setError(`无法打开来源：${String(nextError)}`),
            );
        }}
      >
        {children}
      </a>
      {error && <span role="alert">{error}</span>}
    </>
  );
}
