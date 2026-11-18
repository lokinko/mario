import { dividendBriefs } from "./briefs";
import { dividendReturns, priceOn } from "./returns";
import snapshot from "./snapshot.json";

export type DividendSample = (typeof snapshot.stocks)[number];

const font = '"PingFang SC", "Hiragino Sans GB", "Microsoft YaHei", sans-serif';
const ink = "#172c47";
const muted = "#506581";
const blue = "#24468d";
const left = 84;
const right = 876;

export function wrapPosterText(value: string, fits: (line: string) => boolean) {
  const tokens = value.match(/[A-Za-z0-9.+-]+|./gu) ?? [];
  const lines: string[] = [];
  let line = "";
  for (const token of tokens) {
    if (line && !fits(line + token) && !/^[，。；、？！）]$/.test(token)) {
      lines.push(line.trimEnd());
      line = token.trimStart();
    } else {
      line += token;
    }
  }
  if (line) lines.push(line.trimEnd());
  return lines;
}

export function createDividendPoster(
  stock: DividendSample,
  qr: HTMLCanvasElement,
): Promise<Blob> {
  const canvas = document.createElement("canvas");
  const ctx = canvas.getContext("2d");
  if (!ctx) return Promise.reject(new Error("Canvas unavailable"));
  const [latest, previous] = stock.events;
  const brief = dividendBriefs[stock.symbol];
  const market = dividendReturns(stock.symbol);
  const wrap = (value: string, size: number) => {
    ctx.font = `500 ${size}px ${font}`;
    return wrapPosterText(
      value,
      (line) => ctx.measureText(line).width <= right - left - size,
    );
  };
  const reading = brief ? wrap(brief.reading, 34) : [];
  const followUp = brief ? wrap(brief.followUp, 34) : [];
  const performanceHeight = market ? 416 : 0;
  const analysisTop = 654 + performanceHeight;
  const footerTop = brief
    ? analysisTop + 268 + (reading.length + followUp.length) * 53
    : analysisTop;
  canvas.width = 960;
  canvas.height = footerTop + 350;

  const text = (
    value: string,
    x: number,
    y: number,
    size: number,
    color = ink,
    weight = 500,
  ) => {
    ctx.fillStyle = color;
    ctx.font = `${weight} ${size}px ${font}`;
    ctx.fillText(value, x, y);
  };
  const rule = (y: number) => {
    ctx.strokeStyle = "#c5d4e6";
    ctx.lineWidth = 2;
    ctx.beginPath();
    ctx.moveTo(left, y);
    ctx.lineTo(right, y);
    ctx.stroke();
  };
  const paragraph = (lines: string[], y: number) => {
    lines.forEach((line, index) => text(line, left, y + 53 * index, 34));
  };

  ctx.fillStyle = "#edf3fa";
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  ctx.fillStyle = "#ffffff";
  ctx.beginPath();
  ctx.roundRect(36, 36, 888, canvas.height - 72, 44);
  ctx.fill();
  text("mario  红利观察", left, 114, 33, blue, 700);
  text(stock.name, left, 210, 66, ink, 700);
  text(`A 股 ${stock.symbol}`, left, 264, 29, muted);
  rule(306);

  if (market) {
    text("近 365 天税前股息率", left, 367, 32, muted, 700);
    text(market.yieldPct, left, 475, 96, ink, 750);
    text(`每股派息 ${market.dividendPerShare} 元`, 500, 414, 33, ink, 650);
    text(`不复权收盘价 ${market.close} 元`, 500, 471, 29, muted);
    text(`行情基准日 ${priceOn}`, left, 529, 28, muted);
    rule(565);
    text("历史涨跌（前复权）", left, 619, 35, ink, 700);
    for (const [index, period] of market.periods.entries()) {
      const x = left + index * 267;
      text(period.label, x, 676, 28, muted);
      text(period.value, x, 734, period.from ? 42 : 31, ink, 700);
    }
    rule(306 + performanceHeight + 49);
  }

  const dividendTop = 306 + performanceHeight;
  const dividendOffset = market ? 49 : 0;
  text(
    "最近两次已实施现金分红",
    left,
    dividendTop + dividendOffset + 57,
    36,
    ink,
    700,
  );
  text("除息日", left, dividendTop + dividendOffset + 111, 27, muted);
  text(
    "每 10 股派息（税前）",
    523,
    dividendTop + dividendOffset + 111,
    27,
    muted,
  );
  [latest, previous].forEach((event, index) => {
    if (!event) return;
    const y = dividendTop + dividendOffset + 165 + index * 60;
    text(event.exDate, left, y, 34);
    text(`${event.cashPerTen} 元`, 523, y, 34, ink, 650);
  });
  rule(analysisTop - 33);

  if (brief) {
    text("AI 解读", left, analysisTop + 19, 30, blue, 700);
    text(brief.headline, left, analysisTop + 83, 41, ink, 700);
    paragraph(reading, analysisTop + 148);
    const followTop = analysisTop + 158 + reading.length * 53;
    text("接下来查什么", left, followTop, 32, ink, 700);
    paragraph(followUp, followTop + 54);
    text(
      `预生成解读 · 分红样本截至 ${snapshot.asOf}`,
      left,
      footerTop - 18,
      26,
      muted,
    );
  }

  rule(footerTop + 16);
  text("扫码回到红利观察", left, footerTop + 83, 36, ink, 700);
  text("行情与分红来源  东方财富", left, footerTop + 134, 27, muted);
  if (market) {
    text(
      "股息率 = 365 天税前每股派息 ÷ 收盘价",
      left,
      footerTop + 178,
      24,
      muted,
    );
    text("前复权累计涨跌，未计交易费用", left, footerTop + 217, 24, muted);
  }
  text("历史数据仅供研究参考", left, footerTop + 256, 25, muted);
  ctx.imageSmoothingEnabled = false;
  ctx.drawImage(qr, 634, footerTop + 55, 240, 240);

  return new Promise((resolve, reject) => {
    canvas.toBlob((blob) => {
      if (blob) resolve(blob);
      else reject(new Error("PNG export failed"));
    }, "image/png");
  });
}
