import { Badge } from "../../components/Badge";
import { Callout } from "../../components/Callout";
import { Drawer } from "../../components/Drawer";
import { useEffect, useRef, useState } from "react";
import { Input } from "antd";
import { FileText, Paperclip, Save, Search, Sparkles, X } from "lucide-react";
import {
  getResearchEvidence,
  getResearchAttachment,
  saveResearchEvidence,
  setResearchEvidenceStatus,
  setResearchEvidenceTags,
} from "../../api/evidence";
import type { ResearchEvidence, ResearchEvidenceTags } from "../../types/user";
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

export interface EvidenceStock {
  name: string;
  symbol: string;
}

const noTags: ResearchEvidenceTags = { assetName: "", category: "" };

function stockTag(stock: EvidenceStock) {
  return `${stock.name} ${stock.symbol}`;
}

function TagFields({
  value,
  stocks,
  disabled,
  onChange,
}: {
  value: ResearchEvidenceTags;
  stocks: EvidenceStock[];
  disabled: boolean;
  onChange: (value: ResearchEvidenceTags) => void;
}) {
  const known = stocks.some((stock) => stockTag(stock) === value.assetName);
  return (
    <div className="evidence-tag-fields">
      <label>
        <span>关联股票</span>
        <select
          value={value.assetName}
          disabled={disabled}
          onChange={(event) =>
            onChange({ ...value, assetName: event.target.value })
          }
        >
          <option value="">不关联</option>
          {!known && value.assetName && (
            <option value={value.assetName}>{value.assetName}</option>
          )}
          {stocks.map((stock) => (
            <option key={stock.symbol} value={stockTag(stock)}>
              {stock.name}（{stock.symbol}）
            </option>
          ))}
        </select>
      </label>
      <label>
        <span>信息类型</span>
        <input
          list="evidence-categories"
          value={value.category}
          maxLength={20}
          disabled={disabled}
          placeholder="如 公告、研报、新闻"
          onChange={(event) =>
            onChange({ ...value, category: event.target.value })
          }
        />
      </label>
    </div>
  );
}

export function EvidenceWorkbench({
  stocks,
  onAsk,
  flash,
}: {
  stocks: EvidenceStock[];
  onAsk: (question: string) => void;
  flash: (s: string) => void;
}) {
  const [creating, setCreating] = useState(false);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [newTags, setNewTags] = useState(noTags);
  const [tagDraft, setTagDraft] = useState(noTags);
  const [content, setContent] = useState("");
  const [files, setFiles] = useState<File[]>([]);
  const fileInput = useRef<HTMLInputElement>(null);
  const [downloading, setDownloading] = useState<string | null>(null);
  const [fileError, setFileError] = useState("");
  const [items, setItems] = useState<ResearchEvidence[]>([]);
  const [search, setSearch] = useState("");
  const [stockFilter, setStockFilter] = useState("");
  const [categoryFilter, setCategoryFilter] = useState("");
  const [statusFilter, setStatusFilter] = useState<
    "all" | "active" | "archived"
  >("all");
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
  const categories = [
    ...new Set(items.map((item) => item.category ?? "").filter(Boolean)),
  ];
  const taggedStocks = [
    ...new Set(items.map((item) => item.assetName).filter(Boolean)),
  ];
  const stockName = (assetName: string) =>
    stocks.find((stock) => stockTag(stock) === assetName)?.name ?? assetName;
  const keyword = search.trim().toLocaleLowerCase();
  const filtered = sorted.filter((item) => {
    if (statusFilter === "active" && !item.active) return false;
    if (statusFilter === "archived" && item.active) return false;
    if (stockFilter && item.assetName !== stockFilter) return false;
    if (categoryFilter && (item.category ?? "") !== categoryFilter)
      return false;
    if (!keyword) return true;
    const haystack = [
      evidenceName(item),
      evidenceText(item),
      item.assetName,
      item.category ?? "",
      ...(item.attachments?.map((file) => file.name) ?? []),
    ]
      .join(" ")
      .toLocaleLowerCase();
    return haystack.includes(keyword);
  });
  const hasFilters = Boolean(
    keyword || stockFilter || categoryFilter || statusFilter !== "all",
  );
  const resetFilters = () => {
    setSearch("");
    setStockFilter("");
    setCategoryFilter("");
    setStatusFilter("all");
  };
  const tagsChanged =
    !!selected &&
    (tagDraft.assetName !== selected.assetName ||
      tagDraft.category.trim() !== (selected.category ?? ""));

  const open = (item: ResearchEvidence) => {
    setSelectedId(item.id);
    setTagDraft({ assetName: item.assetName, category: item.category ?? "" });
  };

  const persist = async () => {
    if (saving || (!content.trim() && !files.length)) return;
    setSaving(true);
    setError("");
    try {
      const attachments = await encodeResearchFiles(files);
      await saveResearchEvidence({
        content,
        ...newTags,
        ...(attachments.length ? { attachments } : {}),
      });
      setCreating(false);
      setNewTags(noTags);
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

  const saveTags = async (item: ResearchEvidence) => {
    if (saving) return;
    setSaving(true);
    setError("");
    try {
      const updated = await setResearchEvidenceTags(item.id, tagDraft);
      setItems((current) =>
        current.map((entry) => (entry.id === updated.id ? updated : entry)),
      );
      setTagDraft({
        assetName: updated.assetName,
        category: updated.category ?? "",
      });
      flash("标签已更新");
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
    <>
      <div className="page-actions reference-actions">
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
      </div>
      <datalist id="evidence-categories">
        {categories.map((category) => (
          <option key={category} value={category} />
        ))}
      </datalist>
      {creating && (
        <Drawer
          title="新增资料"
          busy={saving}
          dirty={files.length > 0}
          onClose={() => {
            setCreating(false);
            setNewTags(noTags);
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
              <TagFields
                value={newTags}
                stocks={stocks}
                disabled={saving}
                onChange={setNewTags}
              />
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
      {items.length > 0 && (
        <div
          className="evidence-toolbar"
          role="group"
          aria-label="筛选资料"
          data-presentation="true"
        >
          <div className="evidence-search">
            <Input
              type="search"
              prefix={<Search size={15} aria-hidden="true" />}
              aria-label="搜索参考资料"
              placeholder="搜索标题、正文、标的或附件"
              value={search}
              onChange={(event) => setSearch(event.target.value)}
            />
          </div>
          {taggedStocks.length > 0 && (
            <select
              aria-label="按关联股票筛选"
              value={stockFilter}
              onChange={(event) => setStockFilter(event.target.value)}
            >
              <option value="">全部关联股票</option>
              {taggedStocks.map((asset) => (
                <option key={asset} value={asset}>
                  {stockName(asset)}
                </option>
              ))}
            </select>
          )}
          {categories.length > 0 && (
            <select
              aria-label="按信息类型筛选"
              value={categoryFilter}
              onChange={(event) => setCategoryFilter(event.target.value)}
            >
              <option value="">全部信息类型</option>
              {categories.map((category) => (
                <option key={category} value={category}>
                  {category}
                </option>
              ))}
            </select>
          )}
          <select
            aria-label="按归档状态筛选"
            value={statusFilter}
            onChange={(event) =>
              setStatusFilter(
                event.target.value as "all" | "active" | "archived",
              )
            }
          >
            <option value="all">全部状态</option>
            <option value="active">仅有效资料</option>
            <option value="archived">仅已归档</option>
          </select>
          {hasFilters && (
            <button
              type="button"
              className="text-button"
              onClick={resetFilters}
            >
              清除筛选
            </button>
          )}
        </div>
      )}
      <section className="panel evidence-library" aria-label="资料档案">
        <LoadState
          loading={loading}
          error={creating || selectedId ? "" : error}
          onRetry={() => void refresh()}
        />
        {!loading && !error && items.length === 0 && (
          <p className="empty">还没有资料，点击“新增资料”保存。</p>
        )}
        {!loading && !error && items.length > 0 && filtered.length === 0 && (
          <div className="empty">
            <p>没有符合当前筛选条件的资料。</p>
            <button
              type="button"
              className="text-button"
              onClick={resetFilters}
            >
              清除筛选
            </button>
          </div>
        )}
        <ul className="evidence-files">
          {filtered.map((item) => {
            const name = evidenceName(item);
            return (
              <li key={item.id} className={item.active ? "" : "inactive"}>
                <button
                  type="button"
                  aria-label={name}
                  onClick={() => open(item)}
                >
                  <FileText size={16} aria-hidden />
                  <span className="evidence-file-name">{name}</span>
                  <span className="evidence-file-tags">
                    {item.assetName && (
                      <Badge tone="info">{stockName(item.assetName)}</Badge>
                    )}
                    {item.category && <Badge>{item.category}</Badge>}
                    {!item.active && <Badge>已归档</Badge>}
                  </span>
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
            {selected.evidenceType === "user_source_text" && (
              <div className="evidence-tag-editor">
                <TagFields
                  value={tagDraft}
                  stocks={stocks}
                  disabled={saving}
                  onChange={setTagDraft}
                />
                <button
                  className="secondary"
                  disabled={saving || !tagsChanged}
                  onClick={() => void saveTags(selected)}
                >
                  保存标签
                </button>
              </div>
            )}
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
    </>
  );
}
