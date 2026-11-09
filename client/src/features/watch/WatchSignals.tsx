import type { CSSProperties } from "react";
import type {
  Band,
  BandRule,
  WatchRules,
  WatchMarketRow,
} from "../../api/watch";
import { formatAmount } from "../../lib/format";
import { bandRuleLabel, referenceLabels } from "./rules";
import { checks, distance, number } from "./presentation";

export function RuleChecks({
  row,
  rules,
}: {
  row: WatchMarketRow;
  rules: WatchRules;
}) {
  const values = checks(row);
  return (
    <div className="watch-checks" aria-label="三项观察条件">
      {[
        `股息 ≥${rules.minYieldBps / 100}%`,
        `日线${bandRuleLabel(rules.daily)}`,
        `周线${bandRuleLabel(rules.weekly)}`,
      ].map((label, i) => (
        <span key={label} className={values[i] === true ? "met" : ""}>
          <b aria-hidden="true">
            {values[i] == null ? "?" : values[i] ? "✓" : "·"}
          </b>
          {label}
          <span className="sr-only">
            {values[i] == null ? "未知" : values[i] ? "满足" : "未满足"}
          </span>
        </span>
      ))}
    </div>
  );
}

export function BandTrack({
  label,
  band,
  price,
  rule,
}: {
  label: string;
  band: Band | null | undefined;
  price: number | null | undefined;
  rule: BandRule;
}) {
  const valid =
    band &&
    price != null &&
    price > 0 &&
    band.middle > 0 &&
    band[rule.reference] > 0 &&
    band.upper >= band.middle &&
    band.middle >= band.lower;
  if (!valid)
    return (
      <div className="watch-band watch-band-empty">
        <strong>{label} BOLL</strong>
        <span>轨道资料待补齐</span>
      </div>
    );
  // Visual coordinates only. The reference and rule result come from the server.
  const referenceLabel = referenceLabels[rule.reference];
  const reference = band[rule.reference];
  const offset = rule.offsetBps / 10000;
  const target = reference * (1 + offset);
  const zoneLow =
    rule.mode === "near"
      ? reference * (1 - offset)
      : rule.mode === "at_or_below"
        ? Math.min(band.lower, price, target)
        : target;
  const zoneHigh =
    rule.mode === "near" || rule.mode === "at_or_below"
      ? target
      : Math.max(band.upper, price, target);
  const low = Math.min(band.lower, zoneLow, price, target);
  const high = Math.max(band.upper, zoneHigh, price, target);
  const width = Math.max(high - low, band.middle * 0.04);
  const position = (value: number) =>
    4 + Math.max(0, Math.min(1, (value - low) / width)) * 92;
  const style = {
    "--lower": `${position(band.lower)}%`,
    "--middle": `${position(band.middle)}%`,
    "--upper": `${position(band.upper)}%`,
    "--price": `${position(price)}%`,
    "--zone-start": `${position(zoneLow)}%`,
    "--zone-width": `${position(zoneHigh) - position(zoneLow)}%`,
  } as CSSProperties;
  return (
    <div className={`watch-band ${band.matchesRule ? "near" : ""}`}>
      <div className="watch-band-title">
        <strong>{label} BOLL</strong>
        <span>
          距{referenceLabel} <b>{distance(band.distancePct)}</b>
        </span>
      </div>
      <div
        className="watch-band-track"
        style={style}
        role="img"
        aria-label={`${label}下轨 ${number(band.lower)}，中轨 ${number(band.middle)}，上轨 ${number(band.upper)}，收盘 ${number(price)}；距${referenceLabel} ${distance(band.distancePct)}`}
      >
        <span className="watch-band-zone" />
        <i className="lower" />
        <i className="middle" />
        <i className="upper" />
        <b className="price" />
      </div>
      <div className="watch-band-labels">
        <span>下 {number(band.lower)}</span>
        <span>中 {number(band.middle)}</span>
        <span>上 {number(band.upper)}</span>
      </div>
    </div>
  );
}

export function YieldGrid({
  row,
  minYield,
}: {
  row: WatchMarketRow;
  minYield: number;
}) {
  return (
    <section className="watch-yield-grid" aria-label="股息率参考价格">
      <div className="watch-section-title">
        <strong>股息率参考价</strong>
      </div>
      {row.dividendPrices.length ? (
        <div className="watch-price-cells">
          {row.dividendPrices.map((point) => (
            <div
              key={point.yieldPct}
              className={`${point.yieldPct === minYield ? "threshold" : ""} ${point.reached ? "reached" : ""}`}
            >
              <span>
                {point.yieldPct}%
                {point.yieldPct === minYield && <small>观察门槛</small>}
              </span>
              <strong>¥{formatAmount(point.price)}</strong>
              <small>
                {point.reached
                  ? "股息条件已满足"
                  : point.changePct == null
                    ? "待取得收盘价"
                    : `距参考价 ${distance(point.changePct)}`}
              </small>
            </div>
          ))}
        </div>
      ) : (
        <p>暂无可用参考价：需要有效且为正的 TTM 每股分红。</p>
      )}
    </section>
  );
}
