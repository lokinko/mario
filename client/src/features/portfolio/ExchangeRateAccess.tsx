import { useState } from "react";
import { ArrowLeftRight } from "lucide-react";
import { Drawer } from "../../components/Drawer";
import { ExchangeRates } from "./ExchangeRates";
import { formatNumber } from "../../lib/format";
import type { Snapshot } from "../../types";
export function ExchangeRateAccess({
  snapshot,
  onUpdate,
}: {
  snapshot: Snapshot;
  onUpdate: (snapshot: Snapshot) => void;
}) {
  const [open, setOpen] = useState(false);
  const baseCurrency = snapshot.profile.baseCurrency;
  const currencies = [
    ...new Set(snapshot.holdings.map((h) => h.currency)),
  ].filter((currency) => currency !== baseCurrency);
  return (
    <>
      <button
        className="exchange-access"
        aria-label="查看汇率"
        aria-expanded={open}
        onClick={() => setOpen(true)}
      >
        <ArrowLeftRight size={15} />
        <span>
          {currencies.length
            ? currencies
                .map((currency) => {
                  const rate = snapshot.holdings.find(
                    (h) => h.currency === currency,
                  )?.fxRateToBase;
                  return `${currency}/${baseCurrency} ${rate ? formatNumber(rate, 4, true) : "待补"}`;
                })
                .join(" · ")
            : "汇率与原币核对"}
        </span>
      </button>
      {open && (
        <Drawer title="汇率" onClose={() => setOpen(false)}>
          <ExchangeRates
            baseCurrency={baseCurrency}
            usedCurrencies={currencies}
            onUpdate={onUpdate}
          />
        </Drawer>
      )}
    </>
  );
}
