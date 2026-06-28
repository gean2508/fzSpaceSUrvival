import { useEffect } from "react";

/** Drives CSS vars `--scroll-y`, `--parallax-slow|mid|fast` for layered parallax. */
export function useParallax() {
  useEffect(() => {
    let raf = 0;
    const root = document.documentElement;

    const tick = () => {
      raf = 0;
      const y = window.scrollY;
      root.style.setProperty("--scroll-y", String(y));
      root.style.setProperty("--parallax-slow", `${y * 0.1}px`);
      root.style.setProperty("--parallax-mid", `${y * 0.22}px`);
      root.style.setProperty("--parallax-fast", `${y * 0.38}px`);
    };

    const onScroll = () => {
      if (!raf) raf = requestAnimationFrame(tick);
    };

    window.addEventListener("scroll", onScroll, { passive: true });
    tick();
    return () => {
      window.removeEventListener("scroll", onScroll);
      if (raf) cancelAnimationFrame(raf);
    };
  }, []);
}
