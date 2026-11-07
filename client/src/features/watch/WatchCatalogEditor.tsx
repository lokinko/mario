import { useState } from "react";
import { Button, Input } from "antd";
import {
  getWatchCatalog,
  saveWatchCatalog,
  type CatalogBoard,
} from "../../api/watch";
import { Drawer } from "../../components/Drawer";
import { Callout } from "../../components/Callout";
import { canonical } from "./presentation";

export function WatchCatalogEditor({
  board,
  initialIndustry,
  onClose,
  onEdited,
  onSaved,
}: {
  board: CatalogBoard;
  initialIndustry: string;
  onClose: () => void;
  onEdited: () => void;
  onSaved: (board: CatalogBoard) => void;
}) {
  const [draft, setDraft] = useState(() => structuredClone(board.catalog));
  const [revision, setRevision] = useState(board.revision);
  const [industryId, setIndustryId] = useState(initialIndustry);
  const [selected, setSelected] = useState("");
  const [symbol, setSymbol] = useState("");
  const [name, setName] = useState("");
  const [sources, setSources] = useState("");
  const [dirty, setDirty] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [formDirty, setFormDirty] = useState(false);
  const industry = draft.industries.find((item) => item.id === industryId)!;
  const edited = () => {
    setDirty(true);
    onEdited();
  };
  const reset = () => {
    setSelected("");
    setSymbol("");
    setName("");
    setSources("");
    setFormDirty(false);
  };
  const discardForm = () =>
    !formDirty || window.confirm("当前公司尚未应用修改，确认放弃？");
  const apply = () => {
    const company = {
      symbol: canonical(symbol),
      name: name.trim(),
      sources: sources
        .split(/\r?\n/)
        .map((url) => url.trim())
        .filter(Boolean),
    };
    if (!/^\d{6}\.(SS|SZ)$/.test(company.symbol) || !company.name) {
      setError("请填写沪深 A 股代码和公司名称");
      return;
    }
    if (
      company.sources.length < 2 ||
      company.sources.some((source) => {
        try {
          const url = new URL(source);
          return url.protocol !== "https:" || !url.hostname;
        } catch {
          return true;
        }
      })
    ) {
      setError(
        "请至少填写两个 HTTPS 来源，每行一个：交易所公司资料、公司公开报告",
      );
      return;
    }
    if (
      draft.industries.some((item) =>
        item.companies.some(
          (other) =>
            other.symbol === company.symbol && other.symbol !== selected,
        ),
      )
    ) {
      setError("该公司已在公共目录中，请编辑原有记录");
      return;
    }
    setDraft({
      ...draft,
      industries: draft.industries.map((item) => ({
        ...item,
        companies:
          item.id === industryId
            ? [
                ...item.companies.filter((other) => other.symbol !== selected),
                company,
              ]
            : item.companies.filter((other) => other.symbol !== selected),
      })),
    });
    edited();
    reset();
    setError("");
  };
  const reload = async () => {
    if (
      (dirty || formDirty) &&
      !window.confirm("重新读取将放弃当前目录草稿，确认继续？")
    )
      return;
    setBusy(true);
    try {
      const latest = await getWatchCatalog();
      setDraft(structuredClone(latest.catalog));
      setRevision(latest.revision);
      reset();
      setDirty(false);
      setError("");
    } catch (error) {
      setError(String(error));
    } finally {
      setBusy(false);
    }
  };
  const save = async () => {
    setBusy(true);
    setError("");
    try {
      onSaved(await saveWatchCatalog(draft, revision));
    } catch (error) {
      setError(String(error));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Drawer
      title="维护公共公司目录"
      subtitle="超级账户 · 保存后对所有账号生效"
      size="wide"
      busy={busy}
      resetKey={revision}
      dirty={dirty || formDirty}
      onClose={onClose}
    >
      <div className="watch-catalog-editor">
        <p>
          在领域内增减公司、修改名称及公开来源。移除公共公司会保留用户已有自选和交易。
        </p>
        {error && (
          <Callout tone="danger">
            {error}
            <Button disabled={busy} onClick={() => void reload()}>
              重新读取目录
            </Button>
          </Callout>
        )}
        <label>
          目录核查日期
          <Input
            type="date"
            value={draft.verifiedOn}
            disabled={busy}
            onChange={(event) => {
              setDraft({ ...draft, verifiedOn: event.target.value });
              edited();
            }}
          />
        </label>
        <label>
          领域
          <select
            value={industryId}
            disabled={busy}
            onChange={(event) => {
              if (!discardForm()) return;
              setIndustryId(event.target.value);
              reset();
            }}
          >
            {draft.industries.map((item) => (
              <option key={item.id} value={item.id}>
                {item.name}
              </option>
            ))}
          </select>
        </label>
        <ul className="watch-catalog-companies">
          {industry.companies.map((company) => (
            <li key={company.symbol}>
              <span>
                {company.name} · {company.symbol}
              </span>
              <Button
                disabled={busy}
                onClick={() => {
                  if (!discardForm()) return;
                  setSelected(company.symbol);
                  setSymbol(company.symbol);
                  setName(company.name);
                  setSources(company.sources.join("\n"));
                  setFormDirty(false);
                  setError("");
                }}
              >
                编辑 {company.name}
              </Button>
              <Button
                danger
                disabled={busy}
                onClick={() => {
                  if (
                    !window.confirm(
                      `从全站公共目录移除${company.name}？已加入的个人自选会保留。`,
                    )
                  )
                    return;
                  setDraft({
                    ...draft,
                    industries: draft.industries.map((item) => ({
                      ...item,
                      companies: item.companies.filter(
                        (other) => other.symbol !== company.symbol,
                      ),
                    })),
                  });
                  if (selected === company.symbol) reset();
                  edited();
                }}
              >
                移除 {company.name}
              </Button>
            </li>
          ))}
        </ul>
        {!industry.companies.length && <p>此领域暂无公司，可在下方添加。</p>}
        <form
          onSubmit={(event) => {
            event.preventDefault();
            apply();
          }}
          onChange={() => {
            setFormDirty(true);
            onEdited();
          }}
        >
          <h3>{selected ? "修改公司" : "添加公司"}</h3>
          <label>
            证券代码
            <Input
              value={symbol}
              disabled={busy}
              required
              placeholder="例如 600036.SH"
              onChange={(event) => setSymbol(event.target.value)}
            />
          </label>
          <label>
            公司名称
            <Input
              value={name}
              disabled={busy}
              required
              onChange={(event) => setName(event.target.value)}
            />
          </label>
          <label>
            公开来源（每行一个，至少两个）
            <Input.TextArea
              value={sources}
              disabled={busy}
              required
              rows={3}
              placeholder="交易所公司资料和公司公开报告的 HTTPS 地址"
              onChange={(event) => setSources(event.target.value)}
            />
          </label>
          <div className="watch-actions">
            <Button htmlType="submit" disabled={busy}>
              应用公司修改
            </Button>
            {selected && (
              <Button
                disabled={busy}
                onClick={() => {
                  if (discardForm()) reset();
                }}
              >
                改为添加公司
              </Button>
            )}
          </div>
        </form>
        {formDirty && <p>请先应用当前公司的修改，再保存公共目录。</p>}
        <div className="watch-actions">
          <Button
            type="primary"
            loading={busy}
            disabled={!dirty || formDirty || busy}
            onClick={() => void save()}
          >
            保存公共目录
          </Button>
        </div>
      </div>
    </Drawer>
  );
}
