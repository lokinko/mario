import { expect, it } from "vitest";
import { defaultCloudConfig, resolveBundledCloudConfig } from "./cloudConfig";

it("uses the public product service when no build override is provided", () => {
  expect(resolveBundledCloudConfig({})).toEqual(defaultCloudConfig);
  expect(defaultCloudConfig.publishableKey.startsWith("sb_publishable_")).toBe(
    true,
  );
});

it("uses a complete custom service without mixing partial overrides", () => {
  expect(
    resolveBundledCloudConfig({
      VITE_SUPABASE_URL: " https://custom.supabase.co ",
      VITE_SUPABASE_PUBLISHABLE_KEY: " sb_publishable_custom ",
    }),
  ).toEqual({
    url: "https://custom.supabase.co",
    publishableKey: "sb_publishable_custom",
  });
  expect(
    resolveBundledCloudConfig({
      VITE_SUPABASE_URL: "https://custom.supabase.co",
    }),
  ).toBeNull();
  expect(
    resolveBundledCloudConfig({
      VITE_SUPABASE_PUBLISHABLE_KEY: "sb_publishable_custom",
    }),
  ).toBeNull();
});
