import { QRCode } from "antd";
import { Download, Share2, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { createDividendPoster, type DividendSample } from "./poster";
import { dividendShareUrl } from "./route";

export function DividendShare({
  stock,
  onDismiss,
}: {
  stock: DividendSample;
  onDismiss: () => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const qr = useRef<HTMLDivElement>(null);
  const [poster, setPoster] = useState<{ file: File; href: string }>();
  const [message, setMessage] = useState("");
  const url = dividendShareUrl(stock.symbol);

  useEffect(() => {
    const modal = dialog.current;
    if (modal && !modal.open) modal.showModal();
  }, []);

  useEffect(() => {
    let active = true;
    let href: string | undefined;
    const frame = window.requestAnimationFrame(async () => {
      try {
        const code = qr.current?.querySelector("canvas");
        if (!code) throw new Error("QR unavailable");
        const blob = await createDividendPoster(stock, code);
        if (!active) return;
        const file = new File([blob], `mario-${stock.symbol}.png`, {
          type: "image/png",
        });
        href = URL.createObjectURL(file);
        setPoster({ file, href });
      } catch {
        if (active) setMessage("图片生成失败，可以复制下方链接分享。");
      }
    });
    return () => {
      active = false;
      window.cancelAnimationFrame(frame);
      if (href) URL.revokeObjectURL(href);
    };
  }, [stock]);

  const files = poster ? [poster.file] : [];
  const canShare = !!(
    poster &&
    typeof navigator.share === "function" &&
    navigator.canShare?.({ files })
  );

  return (
    <dialog
      className="dividend-share-dialog"
      ref={dialog}
      onClose={onDismiss}
      aria-labelledby="dividend-share-title"
    >
      <header>
        <h2 id="dividend-share-title">分享 {stock.name} 的分红卡</h2>
        <button
          type="button"
          onClick={() => dialog.current?.close()}
          aria-label="关闭分享卡片"
        >
          <X size={22} aria-hidden="true" />
        </button>
      </header>
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
      {poster && (
        <img
          className="dividend-share-image"
          src={poster.href}
          alt={`${stock.name} 分红卡，含已实施分红、AI 解读摘要和详情二维码`}
        />
      )}
      <div className="dividend-share-options">
        {canShare && (
          <button
            type="button"
            onClick={async () => {
              try {
                await navigator.share({
                  files,
                  title: `${stock.name}分红观察`,
                });
              } catch (error) {
                if (!(error instanceof Error && error.name === "AbortError"))
                  setMessage("系统分享暂不可用，可以保存图片后转发。");
              }
            }}
          >
            <Share2 size={16} aria-hidden="true" /> 分享图片
          </button>
        )}
        {poster && (
          <a href={poster.href} download={poster.file.name}>
            <Download size={16} aria-hidden="true" /> 保存图片
          </a>
        )}
      </div>
      <label className="dividend-share-url">
        卡片链接
        <input
          readOnly
          value={url}
          onFocus={(event) => event.currentTarget.select()}
        />
      </label>
      {message && <p role="status">{message}</p>}
    </dialog>
  );
}
