import { createContext, useContext, useLayoutEffect, useRef } from "react";
import type { HTMLAttributes } from "react";
import { Input } from "antd";
import type { InputProps, InputRef } from "antd";

// The application owns the preference; shared controls only consume its value.
export const AmountPrivacy = createContext(false);

export function Money({
  as: Tag = "span",
  children,
  ...props
}: HTMLAttributes<HTMLElement> & { as?: "span" | "strong" | "td" }) {
  const masked = useContext(AmountPrivacy);
  return (
    <Tag {...props} data-money>
      <span aria-hidden={masked || undefined} data-money-value>
        {children}
      </span>
      {masked && <span className="sr-only">金额已隐藏</span>}
    </Tag>
  );
}

export function PrivateNumberInput(
  props: InputProps & { "data-money-input"?: boolean | string },
) {
  const masked = useContext(AmountPrivacy) && !!props["data-money-input"];
  const ref = useRef<InputRef>(null);
  // Password fields protect the accessible value but have no numeric constraints.
  // Ask the native number control to validate without rendering the real value.
  function validate(input: HTMLInputElement) {
    let message = "";
    if (masked) {
      const number = document.createElement("input");
      number.type = "number";
      for (const key of ["min", "max", "step"] as const) {
        if (props[key] != null) number[key] = String(props[key]);
      }
      number.required = !!props.required;
      number.value = input.value;
      message =
        number.value !== input.value
          ? "请输入有效数字"
          : number.validationMessage;
    }
    input.setCustomValidity(message);
  }
  useLayoutEffect(() => {
    if (ref.current?.input) validate(ref.current.input);
  });
  return (
    <Input
      {...props}
      ref={ref}
      type={masked ? "password" : "number"}
      inputMode="decimal"
      autoComplete={masked ? "off" : props.autoComplete}
      onChange={(event) => {
        validate(event.target);
        props.onChange?.(event);
      }}
    />
  );
}
