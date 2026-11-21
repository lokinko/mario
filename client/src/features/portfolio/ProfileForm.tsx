import { Button } from "antd";
import { Callout } from "../../components/Callout";
import { Save } from "lucide-react";
import { saveProfile } from "../../api/portfolio";
import type {
  FinancialProfile,
  ProfileField,
  Snapshot,
} from "../../types/user";
import { NumberField } from "../../components/NumberField";
import { profileFieldProvided, profileFields } from "./forms";

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
  onSaved,
  flash,
}: {
  profile: FinancialProfile;
  savedBaseCurrency: string;
  onChange: (profile: FinancialProfile) => void;
  busy: boolean;
  run: (task: () => Promise<void>) => Promise<void>;
  onUpdate: (snapshot: Snapshot) => void;
  onSaved?: () => void;
  flash: (message: string) => void;
}) {
  const valid = validProfile(profile);

  const providedFields = (key: ProfileField, value: string) =>
    profileFields.filter((field) =>
      field === key ? value !== "" : profileFieldProvided(profile, field),
    );
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
      providedFields: providedFields(key, value),
      exactAmounts: { ...exact, [key]: value || "0" },
    });
  };
  const amountField = (key: ProfileAmount, label: string) => (
    <NumberField
      label={label}
      value={
        profileFieldProvided(profile, key)
          ? (profile.exactAmounts?.[key] ?? profile[key])
          : ""
      }
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
      const snapshot = await saveProfile({
        ...profile,
        providedFields: profileFields.filter((key) =>
          profileFieldProvided(profile, key),
        ),
      });
      setProfile(snapshot.profile);
      onUpdate(snapshot);
      onSaved?.();
      flash("收支已保存");
    });
  };

  const summaryBits = [
    `记账币种 ${profile.baseCurrency}`,
    profileFieldProvided(profile, "horizonYears")
      ? `期限 ${profile.horizonYears} 年`
      : null,
    profileFieldProvided(profile, "maxDrawdownPct")
      ? `最大回撤 ${profile.maxDrawdownPct}%`
      : null,
  ].filter(Boolean);

  return (
    <section
      className="panel form-panel profile-panel"
      aria-label="收支与风险设置"
    >
      <details className="profile-details">
        <summary className="profile-summary">
          <div className="profile-summary-heading">
            <h2>收入、支出与风险档案</h2>
            <span className="profile-summary-meta">
              {summaryBits.join(" · ")}
            </span>
          </div>
          <span className="profile-summary-action">编辑</span>
        </summary>
        <div className="profile-details-body">
          <div className="profile-currency-row">
            <label>
              <span>记账币种</span>
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
            </label>
          </div>
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
                  value={
                    profileFieldProvided(profile, "horizonYears")
                      ? profile.horizonYears
                      : ""
                  }
                  suffix="年"
                  onChange={(value) =>
                    setProfile({
                      ...profile,
                      horizonYears: value === "" ? 5 : Number(value),
                      providedFields: providedFields("horizonYears", value),
                    })
                  }
                />
                <NumberField
                  label="可承受最大回撤"
                  value={
                    profileFieldProvided(profile, "maxDrawdownPct")
                      ? profile.maxDrawdownPct
                      : ""
                  }
                  suffix="%"
                  onChange={(value) =>
                    setProfile({
                      ...profile,
                      maxDrawdownPct: value === "" ? 15 : Number(value),
                      providedFields: providedFields("maxDrawdownPct", value),
                    })
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
        </div>
      </details>
    </section>
  );
}
