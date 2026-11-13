import type { JSX } from "solid-js";
import { splitProps } from "solid-js";

export interface ButtonProps extends JSX.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: "default" | "primary";
}

/** The one button style. */
export function Button(props: ButtonProps): JSX.Element {
  const [local, rest] = splitProps(props, ["variant", "class"]);
  return (
    <button
      type="button"
      {...rest}
      class={`ep-button ${local.class ?? ""}`}
      data-variant={local.variant ?? "default"}
    />
  );
}
