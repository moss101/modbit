import { forwardRef, type ButtonHTMLAttributes, type ReactNode } from "react";

export type ButtonVariant = "primary" | "secondary" | "ghost" | "danger";
export type ButtonSize = "sm" | "md" | "lg";

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant | undefined;
  size?: ButtonSize | undefined;
}

/** A text button. `type` defaults to "button" so it never submits a form by accident. */
export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button({ variant = "secondary", size = "md", className, type = "button", ...rest }, ref) {
  return <button ref={ref} type={type} className={`mb-btn${className ? ` ${className}` : ""}`} data-variant={variant} data-size={size} {...rest} />;
});

export interface IconButtonProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "aria-label" | "children"> {
  /** The accessible name. Required: an icon has no text of its own. */
  label: string;
  icon: ReactNode;
  /** Toggle state: renders aria-pressed. */
  pressed?: boolean | undefined;
  variant?: ButtonVariant | undefined;
  size?: ButtonSize | undefined;
}

export const IconButton = forwardRef<HTMLButtonElement, IconButtonProps>(function IconButton({ label, icon, pressed, variant = "ghost", size = "md", className, type = "button", ...rest }, ref) {
  return (
    <button ref={ref} type={type} className={`mb-btn mb-icon-btn${className ? ` ${className}` : ""}`} data-variant={variant} data-size={size} aria-label={label} {...(pressed === undefined ? {} : { "aria-pressed": pressed })} {...rest}>
      <span aria-hidden="true" className="mb-icon">
        {icon}
      </span>
    </button>
  );
});
