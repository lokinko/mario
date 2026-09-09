export function NumberField({
  label,
  value,
  onChange,
  prefix,
  suffix,
}: {
  label: string;
  value: number;
  onChange: (v: string) => void;
  prefix?: string;
  suffix?: string;
}) {
  return (
    <label>
      <span>{label}</span>
      <div className="input-affix">
        {prefix && <i>{prefix}</i>}
        <input
          type="number"
          value={value || ""}
          onChange={(e) => onChange(e.target.value)}
        />
        {suffix && <i>{suffix}</i>}
      </div>
    </label>
  );
}
