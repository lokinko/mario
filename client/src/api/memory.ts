import { httpRequest } from "../lib/httpClient";
import type { MemoryCandidate, MemoryPreferenceInput } from "../types";
export async function getMemories(): Promise<MemoryCandidate[]> {
  return httpRequest<MemoryCandidate[]>("/memories");
}

export async function saveMemoryPreference(
  id: string,
  input: MemoryPreferenceInput,
): Promise<MemoryCandidate> {
  return httpRequest<MemoryCandidate>(
    `/memories/${encodeURIComponent(id)}/preference`,
    {
      method: "PUT",
      body: JSON.stringify(input),
    },
  );
}
