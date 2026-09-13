import type { CloudConfig } from "../types";

// Public client configuration, shared with site/auth/auth.js. Access to user
// data is enforced by Supabase authentication and RLS, not by hiding this key.
export const defaultCloudConfig: CloudConfig = {
  url: "https://haqvpuukgsxuroeqohsk.supabase.co",
  publishableKey: "sb_publishable_fPtEQ0_ny5JKVVS5djfGFw_HlzhV0FL",
};

export function resolveBundledCloudConfig(
  environment: Record<string, unknown>,
): CloudConfig | null {
  const url = String(environment.VITE_SUPABASE_URL ?? "").trim();
  const publishableKey = String(
    environment.VITE_SUPABASE_PUBLISHABLE_KEY ?? "",
  ).trim();
  if (!url && !publishableKey) return defaultCloudConfig;
  // Never combine a custom project's URL with another project's key.
  return url && publishableKey ? { url, publishableKey } : null;
}
