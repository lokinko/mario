export function NumberField({
  label,
  value,
  onChange,
  prefix,
  suffix,
}: {
  label: string;
  value: number | string;
  onChange: (value: string) => void;
  prefix?: string;
  suffix?: string;
}) {
  return (
    <label>
      <span>{label}</span>
      <div className="input-affix">
        <input
          data-money-input={
            (prefix && /^[A-Z]{3}$/.test(prefix)) ||
            (suffix && /^[A-Z]{3}$/.test(suffix))
              ? true
              : undefined
          }
          type="number"
          inputMode="decimal"
          step="any"
          value={value}
          onChange={(event) => onChange(event.target.value)}
        />
        {(suffix || prefix) && <i>{suffix || prefix}</i>}
      </div>
    </label>
  );
}
