import { useEffect, useMemo } from "react";

/** A result is publishable only while its request is latest and its view is mounted. */
export function useRequestGuard() {
  const guard = useMemo(() => {
    let revision = 0;
    return {
      begin() {
        const ticket = ++revision;
        return () => ticket === revision;
      },
      invalidate() {
        revision += 1;
      },
    };
  }, []);
  useEffect(() => () => guard.invalidate(), [guard]);
  return guard;
}
