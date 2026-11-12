import { createContext } from "react";

/** The shell tracks protection flags; each editing surface owns its inputs. */
export const DraftProtection = createContext({
  markEdited: (_scope: string) => {},
  clearDraft: (_scope: string) => {},
});
