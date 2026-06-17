import type { ReactNode } from "react";

interface MenuLayoutProps {
  children: ReactNode;
  title?: string;
  onBack?: () => void;
}

/** Shared Ghibli-flavoured backdrop for all menu screens. */
export function MenuLayout({ children, title, onBack }: MenuLayoutProps) {
  return (
    <div className="menu-sky relative h-full w-full overflow-hidden">
      <div className="cloud-layer pointer-events-none absolute inset-0 opacity-70" />
      {/* rolling hills */}
      <div className="pointer-events-none absolute bottom-0 left-0 right-0 h-1/3 bg-[radial-gradient(120%_140%_at_50%_120%,var(--color-meadow)_0%,var(--color-meadow-deep)_70%)]" />

      <div className="relative z-10 flex h-full flex-col items-center justify-center px-5 py-8">
        {onBack && (
          <button
            className="absolute left-4 top-4 rounded-lg bg-black/15 px-3 py-1.5 text-sm font-semibold text-[var(--color-night)] backdrop-blur transition hover:bg-black/25"
            onClick={onBack}
          >
            ◀ Atrás
          </button>
        )}
        {title && (
          <h2 className="float-in mb-6 text-3xl font-extrabold text-[var(--color-night)] drop-shadow">
            {title}
          </h2>
        )}
        {children}
      </div>
    </div>
  );
}

export function PrimaryButton({
  children,
  onClick,
  className = "",
}: {
  children: ReactNode;
  onClick?: () => void;
  className?: string;
}) {
  return (
    <button
      onClick={onClick}
      className={
        "rounded-2xl bg-[var(--color-sunset)] px-8 py-4 text-lg font-extrabold text-white shadow-[0_8px_0_var(--color-bark)] transition active:translate-y-1 active:shadow-[0_4px_0_var(--color-bark)] " +
        className
      }
    >
      {children}
    </button>
  );
}
