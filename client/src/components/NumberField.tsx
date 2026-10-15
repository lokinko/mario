import { useState } from "react";
import { formatNumber } from "../lib/format";

export function NumberField({
  label,
  value,
  onChange,
  prefix,
  suffix,
}: {
  label: string;
  value: number | string;
  onChange: (v: string) => void;
  prefix?: string;
  suffix?: string;
}) {
  const [focused, setFocused] = useState(false);
  return (
    <label>
      <span>{label}</span>
      <div className="input-affix">
        {prefix && <i>{prefix}</i>}
        <input
          type="number"
          step="any"
          value={value ? (focused ? value : formatNumber(value)) : ""}
          onFocus={() => setFocused(true)}
          onBlur={() => setFocused(false)}
          onChange={(e) => onChange(e.target.value)}
        />
        {suffix && <i>{suffix}</i>}
      </div>
    </label>
  );
}
