import { useCallback, useEffect, useRef, useState } from "react";
import { LoaderCircle, Plus, RefreshCw } from "lucide-react";
import { fetchAccountModels } from "../../api/settings";
import type { ModelProviderKind } from "../../types/ai";

export function AccountModelSelect({
  provider,
  baseUrl,
  apiKey,
  value,
  savedModel,
  hasSavedBinding,
  disabled,
  onChange,
}: {
  provider: Exclude<ModelProviderKind, "codex-bridge">;
  baseUrl: string;
  apiKey: string;
  value: string;
  savedModel: string;
  hasSavedBinding: boolean;
  disabled: boolean;
  onChange: (value: string) => void;
}) {
  const normalizedBaseUrl = baseUrl.trim().replace(/\/+$/, "");
  const endpointKey = `${provider} ${normalizedBaseUrl}`;
  const trimmedKey = apiKey.trim();
  const canFetch =
    normalizedBaseUrl.length > 0 && (trimmedKey.length > 0 || hasSavedBinding);

  const [fetchedModels, setFetchedModels] = useState<string[]>([]);
  const [hasFetched, setHasFetched] = useState(false);
  const [customModels, setCustomModels] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [addingCustom, setAddingCustom] = useState(false);
  const [customDraft, setCustomDraft] = useState("");
  const [customError, setCustomError] = useState("");

  const requestIdRef = useRef(0);
  const autoFetchedKeyRef = useRef("");

  const loadModels = useCallback(
    async (keyOverride?: string) => {
      if (!normalizedBaseUrl) return;
      const effectiveKey = (keyOverride ?? trimmedKey) || undefined;
      if (!effectiveKey && !hasSavedBinding) return;
      const requestId = ++requestIdRef.current;
      setLoading(true);
      setError("");
      try {
        const catalog = await fetchAccountModels({
          provider,
          baseUrl: normalizedBaseUrl,
          apiKey: effectiveKey,
        });
        if (requestIdRef.current !== requestId) return;
        setFetchedModels(catalog.models);
        setHasFetched(true);
      } catch (nextError) {
        if (requestIdRef.current !== requestId) return;
        setError(
          nextError instanceof Error ? nextError.message : String(nextError),
        );
      } finally {
        if (requestIdRef.current === requestId) setLoading(false);
      }
    },
    [provider, normalizedBaseUrl, trimmedKey, hasSavedBinding],
  );

  useEffect(() => {
    requestIdRef.current += 1;
    setFetchedModels([]);
    setHasFetched(false);
    setError("");
    setLoading(false);
    autoFetchedKeyRef.current = "";
  }, [endpointKey]);

  useEffect(() => {
    if (
      !hasSavedBinding ||
      trimmedKey.length > 0 ||
      autoFetchedKeyRef.current === endpointKey
    ) {
      return;
    }
    autoFetchedKeyRef.current = endpointKey;
    void loadModels("");
  }, [endpointKey, hasSavedBinding, trimmedKey, loadModels]);

  useEffect(
    () => () => {
      requestIdRef.current += 1;
    },
    [],
  );

  const fetchedSet = new Set(fetchedModels);
  const customOnly = customModels.filter((id) => !fetchedSet.has(id));
  const customSet = new Set(customOnly);
  const showUnlistedValue =
    value.length > 0 && !fetchedSet.has(value) && !customSet.has(value);

  const optionCount =
    (showUnlistedValue ? 1 : 0) + customOnly.length + fetchedModels.length;

  const commitCustomModel = () => {
    const candidate = customDraft.trim();
    if (!candidate) {
      setCustomError("请输入自定义模型名称");
      return;
    }
    if (/\s/.test(candidate)) {
      setCustomError("模型名称不能包含空格");
      return;
    }
    setCustomModels((previous) =>
      previous.includes(candidate) ? previous : [candidate, ...previous],
    );
    onChange(candidate);
    setCustomDraft("");
    setCustomError("");
    setAddingCustom(false);
  };

  return (
    <div className="account-model-picker">
      <label>
        <span>模型名称</span>
        <select
          aria-label="模型名称"
          value={value}
          disabled={disabled || loading}
          onChange={(event) => onChange(event.target.value)}
        >
          <option value="" disabled>
            {loading
              ? "正在获取可用模型…"
              : optionCount > 0
                ? "请选择模型"
                : "请先获取可用模型或添加自定义模型"}
          </option>
          {showUnlistedValue && (
            <option value={value}>
              {value}
              {value === savedModel ? " · 已保存" : " · 自定义"}
            </option>
          )}
          {customOnly.map((id) => (
            <option key={`custom:${id}`} value={id}>
              {id} · 自定义
            </option>
          ))}
          {fetchedModels.map((id) => (
            <option key={id} value={id}>
              {id}
            </option>
          ))}
        </select>
      </label>
      <small className="account-model-hint">
        {loading
          ? "正在从所选接口获取可用模型…"
          : error
            ? error
            : hasFetched
              ? fetchedModels.length > 0
                ? `已获取 ${fetchedModels.length} 个可用模型；如需列表外模型，可添加自定义模型名。`
                : "该接口未返回可用模型，请点击“添加自定义模型”手动添加。"
              : canFetch
                ? "点击“获取可用模型”拉取模型列表，或直接添加自定义模型名。"
                : "请先填写接口地址与密钥以获取可用模型，也可直接添加自定义模型名。"}
      </small>
      <div className="account-model-actions">
        <button
          type="button"
          className="secondary"
          disabled={disabled || loading || !canFetch}
          onClick={() => void loadModels()}
        >
          {loading ? (
            <LoaderCircle size={14} className="spin" />
          ) : (
            <RefreshCw size={14} />
          )}
          {hasFetched ? "重新获取模型" : "获取可用模型"}
        </button>
        <button
          type="button"
          className="secondary"
          disabled={disabled}
          onClick={() => {
            setAddingCustom((open) => !open);
            setCustomError("");
          }}
        >
          <Plus size={14} />
          添加自定义模型
        </button>
      </div>
      {addingCustom && (
        <div className="custom-model-entry">
          <div className="custom-model-row">
            <input
              aria-label="自定义模型名称"
              value={customDraft}
              disabled={disabled}
              placeholder="输入自定义模型 ID，例如 gpt-5.4"
              onChange={(event) => {
                setCustomDraft(event.target.value);
                if (customError) setCustomError("");
              }}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  event.preventDefault();
                  commitCustomModel();
                }
              }}
            />
            <button
              type="button"
              className="secondary"
              disabled={disabled || !customDraft.trim()}
              onClick={commitCustomModel}
            >
              添加
            </button>
            <button
              type="button"
              className="secondary"
              disabled={disabled}
              onClick={() => {
                setAddingCustom(false);
                setCustomDraft("");
                setCustomError("");
              }}
            >
              取消
            </button>
          </div>
          {customError && <small className="field-error">{customError}</small>}
        </div>
      )}
    </div>
  );
}
