import type { ButtonHTMLAttributes, ReactNode } from "react";
import clsx from "clsx";

type Variant = "primary" | "secondary" | "danger";

const variants: Record<Variant, string> = {
  primary: "bg-accent text-accent-fg hover:bg-accent-hover",
  secondary: "border border-line bg-surface text-fg hover:bg-surface-2",
  danger: "bg-danger text-white hover:bg-danger-hover",
};

export function Button({
  variant = "secondary",
  className,
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant }) {
  return (
    <button
      {...props}
      className={clsx(
        "inline-flex items-center justify-center gap-1.5 rounded-md px-3 py-1.5 text-sm font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-50",
        variants[variant],
        className,
      )}
    />
  );
}

export function PageHeader({ title, actions }: { title: string; actions?: ReactNode }) {
  return (
    <header className="flex flex-wrap items-center justify-between gap-3 px-8 pt-7 pb-5">
      <h1 className="text-xl font-semibold tracking-tight">{title}</h1>
      <div className="flex flex-wrap gap-2">{actions}</div>
    </header>
  );
}

export function Card({ className, children }: { className?: string; children: ReactNode }) {
  return <div className={clsx("rounded-xl border border-line bg-surface", className)}>{children}</div>;
}

export const inputClass =
  "w-full rounded-md border border-line bg-bg px-2.5 py-1.5 text-sm outline-none focus:border-accent focus:ring-2 focus:ring-accent/25";
