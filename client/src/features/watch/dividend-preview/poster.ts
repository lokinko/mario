import { dividendBriefs } from "./briefs";
import snapshot from "./snapshot.json";

export type DividendSample = (typeof snapshot.stocks)[number];

export function createDividendPoster(
  stock: DividendSample,
  qr: HTMLCanvasElement,
): Promise<Blob> {
  const canvas = document.createElement("canvas");
  canvas.width = 960;
  const brief = dividendBriefs[stock.symbol];
  const briefHeight = brief ? 120 : 0;
  canvas.height = 1120 + briefHeight;
  const ctx = canvas.getContext("2d");
  if (!ctx) return Promise.reject(new Error("Canvas unavailable"));
  const font =
    '"PingFang SC", "Hiragino Sans GB", "Microsoft YaHei", sans-serif';
  const ink = "#172c47";
  const muted = "#506581";
  const accent = "#24468d";
  const [latest, previous] = stock.events;
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

  ctx.fillStyle = "#edf3fa";
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  ctx.fillStyle = "#ffffff";
  ctx.beginPath();
  ctx.roundRect(36, 36, 888, 1048 + briefHeight, 44);
  ctx.fill();
  ctx.fillStyle = accent;
  ctx.beginPath();
  ctx.arc(98, 105, 13, 0, Math.PI * 2);
  ctx.fill();
  text("mario  红利观察", 128, 118, 33, ink, 700);
  text(`A 股 ${stock.symbol}`, 82, 210, 30, muted);
  text(stock.name, 82, 289, 66, ink, 700);
  text("最近一次已实施现金分红", 82, 370, 32, muted);
  text(latest.cashPerTen, 82, 504, 118, accent, 700);
  text("元 / 10 股（税前）", 84, 561, 34, ink, 600);
  text(`除息日 ${latest.exDate}`, 84, 630, 33);
  if (previous)
    text(
      `上一次 ${previous.exDate}  派 ${previous.cashPerTen} 元 / 10 股`,
      84,
      685,
      29,
      muted,
    );

  if (brief) {
    text("AI 解读摘要", 84, 758, 26, accent, 700);
    text(brief.headline, 84, 810, 38, ink, 700);
  }

  const footerY = 740 + briefHeight;
  ctx.strokeStyle = "#b7c9e1";
  ctx.lineWidth = 2;
  ctx.setLineDash([7, 7]);
  ctx.beginPath();
  ctx.moveTo(82, footerY);
  ctx.lineTo(878, footerY);
  ctx.stroke();
  text(
    brief ? "扫码查看完整解读" : "扫码查看分红明细",
    84,
    footerY + 100,
    36,
    ink,
    700,
  );
  text(`历史样本截至 ${snapshot.asOf}`, 84, footerY + 165, 27, muted);
  text("来源：东方财富分红数据", 84, footerY + 213, 27, muted);
  text("分红记录仅供研究参考", 84, footerY + 261, 27, muted);
  ctx.imageSmoothingEnabled = false;
  ctx.drawImage(qr, 618, footerY + 38, 256, 256);

  return new Promise((resolve, reject) => {
    canvas.toBlob((blob) => {
      if (blob) resolve(blob);
      else reject(new Error("PNG export failed"));
    }, "image/png");
  });
}
