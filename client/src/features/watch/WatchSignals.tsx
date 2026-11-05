import type { CSSProperties } from "react";
import type { Band, WatchRow } from "../../api/watch";
import { formatAmount } from "../../lib/format";
import { checks, distance, number } from "./presentation";

export function RuleChecks({
  row,
  minYield,
}: {
  row: WatchRow;
  minYield: number;
}) {
  const values = checks(row, minYield);
  return (
    <div className="watch-checks" aria-label="三项观察条件">
      {[`股息 ≥${minYield}%`, "日线接近下轨", "周线中轨及以下"].map(
        (label, i) => (
          <span key={label} className={values[i] === true ? "met" : ""}>
            <b aria-hidden="true">
              {values[i] == null ? "?" : values[i] ? "✓" : "·"}
            </b>
            {label}
            <span className="sr-only">
              {values[i] == null ? "未知" : values[i] ? "满足" : "未满足"}
            </span>
          </span>
        ),
      )}
    </div>
  );
}

export function BandTrack({
  label,
  band,
  price,
  tolerance,
}: {
  label: string;
  band: Band | null | undefined;
  price: number | null | undefined;
  tolerance: number;
}) {
  const valid =
    band &&
    price != null &&
    price > 0 &&
    band.middle > 0 &&
    (band.reference === "middle" || band.lower > 0) &&
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
  const weekly = band.reference === "middle";
  const referenceLabel = weekly ? "中轨" : "下轨";
  const low = Math.min(band.lower, band.lower * (1 - tolerance / 100), price);
  const high = Math.max(band.upper, band.lower * (1 + tolerance / 100), price);
  const width = Math.max(high - low, band.middle * 0.04);
  const position = (value: number) =>
    4 + Math.max(0, Math.min(1, (value - low) / width)) * 92;
  const style = {
    "--lower": `${position(band.lower)}%`,
    "--middle": `${position(band.middle)}%`,
    "--upper": `${position(band.upper)}%`,
    "--price": `${position(price)}%`,
    "--zone-start": `${position(weekly ? low : band.lower * (1 - tolerance / 100))}%`,
    "--zone-width": `${position(weekly ? band.middle : band.lower * (1 + tolerance / 100)) - position(weekly ? low : band.lower * (1 - tolerance / 100))}%`,
  } as CSSProperties;
  return (
    <div className={`watch-band ${band.matchesRule ? "near" : ""}`}>
      <div className="watch-band-title">
        <strong>{label} BOLL</strong>
        <span>
          {band.matchesRule ? "在关注范围内" : "未满足"} · 距{referenceLabel}{" "}
          <b>{distance(band.distancePct)}</b>
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

export function YieldGrid({ row }: { row: WatchRow }) {
  return (
    <section className="watch-yield-grid" aria-label="股息率参考价格">
      <div className="watch-section-title">
        <strong>股息率参考价</strong>
        <span>TTM 每股分红 ÷ 目标股息率</span>
      </div>
      {row.dividendPrices.length ? (
        <div className="watch-price-cells">
          {row.dividendPrices.map((point) => (
            <div
              key={point.yieldPct}
              className={`${point.yieldPct === 5 ? "threshold" : ""} ${point.reached ? "reached" : ""}`}
            >
              <span>
                {point.yieldPct}%
                {point.yieldPct === 5 && <small>观察门槛</small>}
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
      <p className="watch-meta">
        按 {row.latest?.observedOn ?? "—"}{" "}
        的已实施分红口径，参考价向下取至分。仅比较股息条件，日周 BOLL
        仍须同时满足。
      </p>
    </section>
  );
}
