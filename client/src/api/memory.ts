import { httpRequest } from "../lib/httpClient";
import type { MemoryNote, MemoryOverview } from "../types/ai";

export async function getMemory(): Promise<MemoryOverview> {
  return httpRequest<MemoryOverview>("/memory");
}

/** Consolidate recent analyses into memory; the server screens every proposed change. */
export async function dreamMemory(): Promise<MemoryOverview> {
  return httpRequest<MemoryOverview>("/memory/dream", { method: "POST" });
}

export async function setMemoryStatus(
  id: string,
  active: boolean,
): Promise<MemoryNote> {
  return httpRequest<MemoryNote>(`/memory/${encodeURIComponent(id)}/status`, {
    method: "PUT",
    body: JSON.stringify({ active }),
  });
}

export async function deleteMemory(id: string): Promise<void> {
  await httpRequest<void>(`/memory/${encodeURIComponent(id)}`, {
    method: "DELETE",
  });
}
