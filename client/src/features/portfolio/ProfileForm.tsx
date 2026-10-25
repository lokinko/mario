import { Callout } from "../../components/Callout";
import { useId } from "react";
import { Save } from "lucide-react";
import { saveProfile } from "../../api/portfolio";
import type { FinancialProfile, Snapshot } from "../../types";
import { NumberField } from "../../components/NumberField";

type ProfileAmount =
  | "monthlyIncome"
  | "monthlyExpense"
  | "emergencyFund"
  | "liabilities"
  | "investableAssets";

function validProfile(profile: FinancialProfile) {
  return (
    [
      profile.monthlyIncome,
      profile.monthlyExpense,
      profile.liabilities,
      profile.emergencyFund,
      profile.investableAssets,
    ].every((value) => Number.isFinite(value) && value >= 0 && value <= 1e15) &&
    Number.isInteger(profile.horizonYears) &&
    profile.horizonYears >= 0 &&
    profile.horizonYears <= 80 &&
    Number.isFinite(profile.maxDrawdownPct) &&
    profile.maxDrawdownPct >= 0 &&
    profile.maxDrawdownPct <= 100
  );
}

export function ProfileForm({
  profile,
  onChange: setProfile,
  busy,
  run,
  onUpdate,
  flash,
}: {
  profile: FinancialProfile;
  onChange: (profile: FinancialProfile) => void;
  busy: boolean;
  run: (task: () => Promise<void>) => Promise<void>;
  onUpdate: (snapshot: Snapshot) => void;
  flash: (message: string) => void;
}) {
  const currencyHelpId = useId();
  const valid = validProfile(profile);

  const setAmount = (key: ProfileAmount, value: string) => {
    const exact = profile.exactAmounts ?? {
      monthlyIncome: String(profile.monthlyIncome),
      monthlyExpense: String(profile.monthlyExpense),
      emergencyFund: String(profile.emergencyFund),
      liabilities: String(profile.liabilities),
      investableAssets: String(profile.investableAssets),
    };
    setProfile({
      ...profile,
      [key]: Number(value),
      exactAmounts: { ...exact, [key]: value || "0" },
    });
  };
  const amountField = (key: ProfileAmount, label: string) => (
    <NumberField
      label={label}
      value={profile.exactAmounts?.[key] ?? profile[key]}
      onChange={(value) => setAmount(key, value)}
      prefix={profile.baseCurrency}
    />
  );
  const save = () => {
    if (busy || !valid) return;
    void run(async () => {
      onUpdate(await saveProfile(profile));
      flash("收支已保存");
    });
  };

  return (
    <section className="panel form-panel" aria-label="收支与风险设置">
      <h2>收入与支出</h2>
      <p className="section-intro">
        记录稳定月度现金流，作为风险承受能力的基础。
      </p>
      {!valid && (
        <Callout tone="warning">
          请核对金额和投资偏好：金额不能为负，投资期限为 0—80 年，回撤为
          0—100%。
        </Callout>
      )}
      <form
        onSubmit={(event) => {
          event.preventDefault();
          save();
        }}
      >
        <fieldset className="holding-fields" disabled={busy}>
          <div className="form-grid">
            {amountField("monthlyIncome", "月收入")}
            {amountField("monthlyExpense", "月支出")}
          </div>
          <details className="holding-details">
            <summary>有负债或预留备用金？（选填）</summary>
            <div className="form-grid">
              {amountField("liabilities", "尚未还清的负债")}
              {amountField("emergencyFund", "存款中预留的备用金")}
            </div>
            <p className="section-intro">
              备用金是存款中留作急用的部分，不会再次计入资产。
            </p>
          </details>
          <details className="holding-details">
            <summary>投资偏好与记账币种</summary>
            <div className="form-grid">
              <label>
                <span>基准币种</span>
                <select
                  aria-label="基准币种"
                  aria-describedby={currencyHelpId}
                  value={profile.baseCurrency}
                  onChange={(event) =>
                    setProfile({ ...profile, baseCurrency: event.target.value })
                  }
                >
                  {["CNY", "USD", "HKD", "EUR", "JPY", "GBP"].map((value) => (
                    <option key={value}>{value}</option>
                  ))}
                </select>
                <small id={currencyHelpId}>
                  汇总和目标使用此币种；切换不会自动换算已填金额。
                </small>
              </label>
              {amountField("investableAssets", "可投资资产")}
              <NumberField
                label="投资期限"
                value={profile.horizonYears}
                suffix="年"
                onChange={(value) =>
                  setProfile({ ...profile, horizonYears: Number(value) })
                }
              />
              <NumberField
                label="可承受最大回撤"
                value={profile.maxDrawdownPct}
                suffix="%"
                onChange={(value) =>
                  setProfile({ ...profile, maxDrawdownPct: Number(value) })
                }
              />
              <label>
                <span>风险倾向</span>
                <select
                  value={profile.riskLevel}
                  onChange={(event) =>
                    setProfile({
                      ...profile,
                      riskLevel: event.target
                        .value as FinancialProfile["riskLevel"],
                    })
                  }
                >
                  {["保守", "稳健", "均衡", "进取"].map((value) => (
                    <option key={value}>{value}</option>
                  ))}
                </select>
              </label>
            </div>
          </details>
          <div className="form-actions">
            <button className="primary" type="submit" disabled={!valid}>
              <Save size={16} />
              {busy ? "保存中…" : "保存收支"}
            </button>
          </div>
        </fieldset>
      </form>
    </section>
  );
}
