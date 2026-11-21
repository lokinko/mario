import { Button, Input, QRCode } from "antd";
import { Download, Share2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Callout } from "../../../components/Callout";
import { Drawer } from "../../../components/Drawer";
import type { DividendCard } from "./card";
import { createDividendPoster } from "./poster";
import { dividendShareUrl } from "./route";

export function DividendShare({
  card,
  onDismiss,
}: {
  card: DividendCard;
  onDismiss: () => void;
}) {
  const qr = useRef<HTMLDivElement>(null);
  const posterUrl = useRef<string | undefined>(undefined);
  const [includeQr, setIncludeQr] = useState(false);
  const [poster, setPoster] = useState<{
    file: File;
    href: string;
    includeQr: boolean;
  }>();
  const [message, setMessage] = useState("");
  const url = dividendShareUrl(card.symbol);

  useEffect(
    () => () => {
      if (posterUrl.current) URL.revokeObjectURL(posterUrl.current);
    },
    [],
  );

  useEffect(() => {
    let active = true;
    const frame = window.requestAnimationFrame(async () => {
      try {
        let code: HTMLCanvasElement | undefined;
        if (includeQr) {
          code = qr.current?.querySelector("canvas") ?? undefined;
          if (!code) throw new Error("QR unavailable");
        }
        const blob = await createDividendPoster(card, code);
        if (!active) return;
        const file = new File([blob], `mario-${card.symbol}.png`, {
          type: "image/png",
        });
        const href = URL.createObjectURL(file);
        const previous = posterUrl.current;
        posterUrl.current = href;
        setMessage("");
        setPoster({ file, href, includeQr });
        if (previous) URL.revokeObjectURL(previous);
      } catch {
        if (active) setMessage("图片生成失败，可以复制下方链接分享。");
      }
    });
    return () => {
      active = false;
      window.cancelAnimationFrame(frame);
    };
  }, [card, includeQr]);

  const files = poster ? [poster.file] : [];
  const canShare = !!(
    poster &&
    typeof navigator.share === "function" &&
    navigator.canShare?.({ files })
  );

  const headings = card.sections.map(({ title }) => title).join("、");

  return (
    <Drawer title={`分享 ${card.name} 的分红卡`} onClose={onDismiss}>
      {includeQr && (
        <div ref={qr} className="dividend-share-qr" hidden={!!poster}>
          <QRCode
            value={url}
            size={256}
            bordered={false}
            color="#172c47"
            bgColor="#ffffff"
            marginSize={4}
            aria-label="扫码打开分红详情"
          />
        </div>
      )}
      {poster && (
        <div
          className="dividend-share-preview"
          role="region"
          aria-label="长图预览"
          tabIndex={0}
        >
          <img
            className="dividend-share-image"
            src={poster.href}
            alt={
              poster.includeQr
                ? `${card.name} 红利长图，含${headings}和详情二维码。`
                : `${card.name} 红利长图，含${headings}。`
            }
          />
        </div>
      )}
      <label className="dividend-share-toggle" data-presentation="true">
        <input
          type="checkbox"
          checked={includeQr}
          onChange={(event) => setIncludeQr(event.target.checked)}
        />
        导出时附带二维码
      </label>
      <div className="dividend-share-options">
        {canShare && (
          <Button
            type="primary"
            icon={<Share2 size={16} aria-hidden="true" />}
            onClick={async () => {
              try {
                await navigator.share({
                  files,
                  title: `${card.name}分红观察`,
                });
              } catch (error) {
                if (!(error instanceof Error && error.name === "AbortError"))
                  setMessage("系统分享暂不可用，可以保存图片后转发。");
              }
            }}
          >
            分享图片
          </Button>
        )}
        {poster && (
          <Button
            href={poster.href}
            download={poster.file.name}
            icon={<Download size={16} aria-hidden="true" />}
          >
            保存图片
          </Button>
        )}
      </div>
      <label className="dividend-share-url">
        卡片链接
        <Input
          readOnly
          value={url}
          onFocus={(event) => event.currentTarget.select()}
        />
      </label>
      {message && <Callout tone="warning">{message}</Callout>}
    </Drawer>
  );
}
