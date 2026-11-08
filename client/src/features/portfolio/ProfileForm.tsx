import { Button } from "antd";
import { Callout } from "../../components/Callout";
import { Save } from "lucide-react";
import { saveProfile } from "../../api/portfolio";
import type { FinancialProfile, Snapshot } from "../../types";
import { NumberField } from "../../components/NumberField";

type ProfileAmount =
  "monthlyIncome" | "monthlyExpense" | "emergencyFund" | "liabilities";

function validProfile(profile: FinancialProfile) {
  return (
    [
      profile.monthlyIncome,
      profile.monthlyExpense,
      profile.liabilities,
      profile.emergencyFund,
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
  savedBaseCurrency,
  onChange: setProfile,
  busy,
  run,
  onUpdate,
  flash,
}: {
  profile: FinancialProfile;
  savedBaseCurrency: string;
  onChange: (profile: FinancialProfile) => void;
  busy: boolean;
  run: (task: () => Promise<void>) => Promise<void>;
  onUpdate: (snapshot: Snapshot) => void;
  flash: (message: string) => void;
}) {
  const valid = validProfile(profile);

  const setAmount = (key: ProfileAmount, value: string) => {
    const exact = profile.exactAmounts ?? {
      monthlyIncome: String(profile.monthlyIncome),
      monthlyExpense: String(profile.monthlyExpense),
      emergencyFund: String(profile.emergencyFund),
      liabilities: String(profile.liabilities),
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
    if (
      profile.baseCurrency !== savedBaseCurrency &&
      !window.confirm(
        `币种将从 ${savedBaseCurrency} 改为 ${profile.baseCurrency}，总资产等汇总也会改用该币种，已填金额不会自动换算。继续？`,
      )
    )
      return;
    void run(async () => {
      onUpdate(await saveProfile(profile));
      flash("收支已保存");
    });
  };

  return (
    <section
      className="panel form-panel profile-panel"
      aria-label="收支与风险设置"
    >
      <div className="panel-title">
        <h2>收入与支出</h2>
        <select
          aria-label="记账币种"
          value={profile.baseCurrency}
          disabled={busy}
          onChange={(event) =>
            setProfile({ ...profile, baseCurrency: event.target.value })
          }
        >
          {["CNY", "USD", "HKD", "EUR", "JPY", "GBP"].map((value) => (
            <option key={value}>{value}</option>
          ))}
        </select>
      </div>
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
        className="portfolio-form"
        onSubmit={(event) => {
          event.preventDefault();
          save();
        }}
      >
        <fieldset className="holding-fields" disabled={busy}>
          <div className="form-grid">
            {amountField("monthlyIncome", "月收入")}
            {amountField("monthlyExpense", "月支出")}
            {amountField("liabilities", "负债")}
            {amountField("emergencyFund", "备用金")}
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
          </div>
          <div className="form-actions">
            <Button
              type="primary"
              htmlType="submit"
              disabled={busy || !valid}
              icon={<Save size={15} />}
            >
              {busy ? "保存中…" : "保存收支"}
            </Button>
          </div>
        </fieldset>
      </form>
    </section>
  );
}
