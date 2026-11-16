import { Badge } from "../../components/Badge";
import { Callout } from "../../components/Callout";
import { Drawer } from "../../components/Drawer";
import { useEffect, useRef, useState } from "react";
import { FileText, Paperclip, Save, Sparkles, X } from "lucide-react";
import {
  getResearchEvidence,
  getResearchAttachment,
  saveResearchEvidence,
  setResearchEvidenceStatus,
} from "../../api/research";
import type { ResearchEvidence } from "../../types";
import { PageHeader } from "../../components/PageHeader";
import { LoadState } from "../../components/LoadState";
import { saveAttachmentDownload } from "../../lib/download";
import { localDateValue } from "../../lib/dates";
import { useRequestGuard } from "../../lib/useRequestGuard";
import {
  encodeResearchFiles,
  formatFileSize,
  validateResearchFiles,
} from "../../lib/researchFiles";

export function evidenceText(item: ResearchEvidence): string {
  if (item.evidenceType === "user_source_text") return item.claim;
  return [
    item.assetName && `主题：${item.assetName}`,
    item.title,
    item.publisher && `来源：${item.publisher}`,
    item.sourceUrl,
    item.asOfDate && `资料日期：${item.asOfDate}`,
    item.sourceTier && `来源层级：${item.sourceTier}`,
    item.evidenceType && `资料类型：${item.evidenceType}`,
    item.stance && `与原假设的关系：${item.stance}`,
    item.claim,
    item.notes && `限制与待核实项：${item.notes}`,
  ]
    .filter(Boolean)
    .join("\n\n");
}

function evidenceName(item: ResearchEvidence) {
  return (
    item.title ||
    item.claim
      ?.split("\n")
      .find((line) => line.trim())
      ?.trim()
      .slice(0, 72) ||
    item.attachments?.[0]?.name ||
    "参考资料"
  );
}

export function EvidenceWorkbench({
  onAsk,
  flash,
}: {
  onAsk: (question: string) => void;
  flash: (s: string) => void;
}) {
  const [creating, setCreating] = useState(false);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [content, setContent] = useState("");
  const [files, setFiles] = useState<File[]>([]);
  const fileInput = useRef<HTMLInputElement>(null);
  const [downloading, setDownloading] = useState<string | null>(null);
  const [fileError, setFileError] = useState("");
  const [items, setItems] = useState<ResearchEvidence[]>([]);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(true);
  const loadRequest = useRequestGuard();

  const refresh = async () => {
    const isCurrent = loadRequest.begin();
    setLoading(true);
    try {
      const nextItems = await getResearchEvidence();
      if (!isCurrent()) return;
      setItems(nextItems);
      setError("");
    } catch (nextError) {
      if (isCurrent()) setError(String(nextError));
    } finally {
      if (isCurrent()) setLoading(false);
    }
  };

  useEffect(() => {
    void refresh();
  }, []);

  const sorted = [...items].sort((a, b) =>
    b.capturedAt.localeCompare(a.capturedAt),
  );
  const selected = items.find((item) => item.id === selectedId);
  const active = items.filter((item) => item.active);

  const persist = async () => {
    if (saving || (!content.trim() && !files.length)) return;
    setSaving(true);
    setError("");
    try {
      const attachments = await encodeResearchFiles(files);
      await saveResearchEvidence({
        content,
        ...(attachments.length ? { attachments } : {}),
      });
      setCreating(false);
      setContent("");
      setFiles([]);
      setFileError("");
      flash("参考资料已保存");
      await refresh();
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setSaving(false);
    }
  };

  const addFiles = (selected: File[]) => {
    const next = [...files, ...selected];
    const message = validateResearchFiles(next);
    setFileError(message);
    if (!message) setFiles(next);
  };

  const download = async (evidenceId: string, attachmentId: string) => {
    if (downloading) return;
    setDownloading(attachmentId);
    setError("");
    try {
      const file = await getResearchAttachment(evidenceId, attachmentId);
      await saveAttachmentDownload(file);
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setDownloading(null);
    }
  };

  const toggleStatus = async (item: ResearchEvidence) => {
    if (saving) return;
    setSaving(true);
    setError("");
    try {
      await setResearchEvidenceStatus(item.id, !item.active);
      await refresh();
      setSelectedId(null);
      flash(item.active ? "资料已归档" : "资料已恢复");
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setSaving(false);
    }
  };

  const startEvidenceAnalysis = () => {
    const topics = active
      .slice(0, 12)
      .map((item) => item.title)
      .filter(Boolean)
      .join("、");
    onAsk(
      `请结合我保存的原始资料与资讯${topics ? `（${topics}）` : ""}，梳理其中的重要信息、局限和待核实事项，再分析它们对当前投资判断的影响。引用实际提供的资料，不要补造来源信息。`,
    );
  };

  return (
    <div className="page narrow">
      <PageHeader
        title="参考资料"
        action={
          <>
            <button className="primary" onClick={() => setCreating(true)}>
              新增资料
            </button>
            <button
              className="secondary"
              onClick={startEvidenceAnalysis}
              disabled={!active.length}
            >
              <Sparkles size={16} />
              用资料提问
            </button>
          </>
        }
      />
      {creating && (
        <Drawer
          title="新增资料"
          busy={saving}
          dirty={files.length > 0}
          onClose={() => {
            setCreating(false);
            setContent("");
            setFiles([]);
            setFileError("");
            setError("");
          }}
        >
          {error && <Callout tone="danger">{error}</Callout>}
          <section className="evidence-form">
            <form
              onSubmit={(event) => {
                event.preventDefault();
                void persist();
              }}
            >
              <label className="evidence-content-field">
                <span>原始资料与资讯</span>
                <textarea
                  rows={12}
                  required={!files.length}
                  disabled={saving}
                  maxLength={20000}
                  value={content}
                  onChange={(event) => setContent(event.target.value)}
                  placeholder="直接粘贴或写下你获取到的重要原始信息。来源、链接和日期也可以一起放在这段文字里。"
                />
              </label>
              <input
                ref={fileInput}
                type="file"
                multiple
                hidden
                aria-label="选择附件"
                disabled={saving}
                onChange={(event) => {
                  addFiles(Array.from(event.target.files ?? []));
                  event.target.value = "";
                }}
              />
              {files.length > 0 && (
                <ul className="research-attachments" aria-label="待保存的附件">
                  {files.map((file, index) => (
                    <li key={`${file.name}-${index}`}>
                      <span>
                        {file.name} <small>{formatFileSize(file.size)}</small>
                      </span>
                      <button
                        type="button"
                        className="text-button"
                        disabled={saving}
                        aria-label={`移除附件 ${file.name}`}
                        onClick={() => {
                          setFiles(files.filter((_, i) => i !== index));
                          setFileError("");
                        }}
                      >
                        <X size={16} />
                      </button>
                    </li>
                  ))}
                </ul>
              )}
              {fileError && <Callout tone="danger">{fileError}</Callout>}
              <div className="form-actions">
                <button
                  type="button"
                  className="secondary"
                  disabled={saving}
                  onClick={() => fileInput.current?.click()}
                >
                  <Paperclip size={16} />
                  添加文件
                </button>
                <small>最多 5 个文件，总计 5 MB</small>
                <button
                  className="primary"
                  type="submit"
                  disabled={saving || (!content.trim() && !files.length)}
                >
                  <Save size={16} />
                  {saving ? "保存中…" : "保存资料"}
                </button>
              </div>
            </form>
          </section>
        </Drawer>
      )}
      <section className="panel evidence-library" aria-label="资料档案">
        <LoadState
          loading={loading}
          error={creating || selectedId ? "" : error}
          onRetry={() => void refresh()}
        />
        {!loading && !error && items.length === 0 && (
          <p className="empty">还没有资料，点击右上角“新增资料”保存。</p>
        )}
        <ul className="evidence-files">
          {sorted.map((item) => {
            const name = evidenceName(item);
            return (
              <li key={item.id} className={item.active ? "" : "inactive"}>
                <button
                  type="button"
                  aria-label={name}
                  onClick={() => setSelectedId(item.id)}
                >
                  <FileText size={16} aria-hidden />
                  <span className="evidence-file-name">{name}</span>
                  <span>{!item.active && <Badge>已归档</Badge>}</span>
                  <small>
                    {item.attachments?.length
                      ? `${item.attachments.length} 个附件`
                      : ""}
                  </small>
                  <time dateTime={item.capturedAt}>
                    {localDateValue(new Date(item.capturedAt))}
                  </time>
                </button>
              </li>
            );
          })}
        </ul>
        {selected && (
          <Drawer
            title={evidenceName(selected)}
            busy={saving || !!downloading}
            onClose={() => setSelectedId(null)}
          >
            {error && <Callout tone="danger">{error}</Callout>}
            <small className="evidence-saved-on">
              保存于 {localDateValue(new Date(selected.capturedAt))}
              {!selected.active && " · 已归档"}
            </small>
            {evidenceText(selected) && (
              <p className="evidence-content">{evidenceText(selected)}</p>
            )}
            {!!selected.attachments?.length && (
              <ul className="research-attachments" aria-label="已保存的附件">
                {selected.attachments.map((file) => (
                  <li key={file.id}>
                    <div>
                      <button
                        type="button"
                        className="text-button"
                        disabled={!!downloading}
                        onClick={() => void download(selected.id, file.id)}
                      >
                        <Paperclip size={14} />
                        {downloading === file.id ? "下载中…" : file.name}
                      </button>
                      <small>
                        {formatFileSize(file.byteSize)} · {file.extractionNote}
                      </small>
                    </div>
                  </li>
                ))}
              </ul>
            )}
            <button
              className="secondary"
              disabled={saving}
              onClick={() => void toggleStatus(selected)}
            >
              {selected.active ? "归档" : "恢复"}
            </button>
          </Drawer>
        )}
      </section>
    </div>
  );
}
