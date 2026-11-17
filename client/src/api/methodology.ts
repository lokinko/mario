import { httpRequest } from "../lib/httpClient";
import type { InvestmentMethodology } from "../types/user";
export function getInvestmentMethodology(): Promise<InvestmentMethodology> {
  return httpRequest<InvestmentMethodology>("/investment-methodology");
}

export function saveInvestmentMethodology(
  prompt: string,
): Promise<InvestmentMethodology> {
  return httpRequest<InvestmentMethodology>("/investment-methodology", {
    method: "PUT",
    body: JSON.stringify({ prompt }),
  });
}
