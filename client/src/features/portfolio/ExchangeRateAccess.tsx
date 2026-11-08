import { useState } from "react";
import { Button } from "antd";
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
  const rates = currencies
    .map((currency) => {
      const rate = snapshot.holdings.find(
        (h) => h.currency === currency,
      )?.fxRateToBase;
      return `${currency}/${baseCurrency} ${rate ? formatNumber(rate, 4, true) : "待补"}`;
    })
    .join(" · ");
  return (
    <>
      <Button
        icon={<ArrowLeftRight size={15} />}
        aria-label="查看汇率"
        aria-expanded={open}
        title={rates || "汇率与原币核对"}
        onClick={() => setOpen(true)}
      >
        汇率
      </Button>
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
