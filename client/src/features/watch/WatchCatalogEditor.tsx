import { useState } from "react";
import { Button, Input } from "antd";
import {
  getWatchCatalog,
  saveWatchCatalog,
  resolveCatalogCompany,
  type CatalogBoard,
} from "../../api/watch";
import { Drawer } from "../../components/Drawer";
import { Callout } from "../../components/Callout";

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
  const [query, setQuery] = useState("");
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
    setQuery("");
    setFormDirty(false);
  };
  const discardForm = () =>
    !formDirty || window.confirm("当前公司尚未应用修改，确认放弃？");
  const apply = async () => {
    setBusy(true);
    setError("");
    try {
      const company = await resolveCatalogCompany(query.trim());
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
                  ...item.companies.filter(
                    (other) => other.symbol !== selected,
                  ),
                  company,
                ]
              : item.companies.filter((other) => other.symbol !== selected),
        })),
      });
      edited();
      reset();
    } catch (error) {
      setError(String(error));
    } finally {
      setBusy(false);
    }
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
          输入公司名称或证券代码即可添加，公开来源由系统查找，缺失时自动补充。移除公共公司会保留用户已有自选和交易。
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
                  setQuery(company.symbol);
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
            void apply();
          }}
          onChange={() => {
            setFormDirty(true);
            onEdited();
          }}
        >
          <h3>{selected ? "修改公司" : "添加公司"}</h3>
          <label>
            公司名称或证券代码
            <Input
              value={query}
              disabled={busy}
              required
              placeholder="例如 国电电力 或 600795"
              onChange={(event) => setQuery(event.target.value)}
            />
          </label>
          <div className="watch-actions">
            <Button htmlType="submit" loading={busy} disabled={busy}>
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
