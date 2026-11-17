export interface FxRateQuote {
  exactRate?: string | null;
  fromCurrency: string;
  toCurrency: string;
  rate: number;
  requestedOn: string;
  observedOn: string;
  stalenessDays: number;
  providerCode: string;
  providerName: string;
  sourceUrl: string;
  methodologyUrl: string;
  disclaimer: string;
}

export interface SecurityPriceQuote {
  symbol: string;
  currency: string;
  close: number;
  requestedOn: string;
  observedOn: string;
  stalenessDays: number;
  providerCode: string;
  providerName: string;
  exchange: string;
  micCode: string;
  instrumentType: string;
  priceBasis: string;
  sourceUrl: string;
  methodologyUrl: string;
  disclaimer: string;
}
