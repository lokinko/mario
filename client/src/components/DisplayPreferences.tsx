export type ThemePreference = "system" | "light" | "dark";

export interface DisplayPreferenceProps {
  pnl: "cn" | "intl";
  maskAmounts: boolean;
  theme: ThemePreference;
  setPnl: (pnl: "cn" | "intl") => void;
  setMaskAmounts: (mask: boolean) => void;
  setTheme: (theme: ThemePreference) => void;
}
export function DisplayPreferences({
  pnl,
  maskAmounts,
  theme,
  setPnl,
  setMaskAmounts,
  setTheme,
}: DisplayPreferenceProps) {
  return (
    <section
      className="panel form-panel"
      id="display-settings"
      data-presentation="true"
    >
      <div className="panel-title">
        <h2>显示偏好</h2>
      </div>
      <div className="form-grid">
        <label>
          <span>外观主题</span>
          <select
            value={theme}
            onChange={(event) =>
              setTheme(event.target.value as ThemePreference)
            }
          >
            <option value="system">跟随系统</option>
            <option value="light">浅色</option>
            <option value="dark">深色</option>
          </select>
        </label>
        <label>
          <span>涨跌色</span>
          <select
            value={pnl}
            onChange={(event) => setPnl(event.target.value as "cn" | "intl")}
          >
            <option value="cn">红涨绿跌</option>
            <option value="intl">绿涨红跌</option>
          </select>
        </label>
        <label>
          <span>金额显示</span>
          <select
            value={maskAmounts ? "hidden" : "visible"}
            onChange={(event) =>
              setMaskAmounts(event.target.value === "hidden")
            }
          >
            <option value="visible">显示金额</option>
            <option value="hidden">隐藏金额</option>
          </select>
        </label>
      </div>
    </section>
  );
}
