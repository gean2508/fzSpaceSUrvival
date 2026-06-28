import { useEffect, useRef } from "react";

/** Adds `.reveal--visible` when elements enter the viewport. Re-scans on DOM updates. */
export function useScrollReveal<T extends HTMLElement>() {
  const ref = useRef<T | null>(null);

  useEffect(() => {
    const root = ref.current;
    if (!root) return;

    const io = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) {
            entry.target.classList.add("reveal--visible");
            io.unobserve(entry.target);
          }
        }
      },
      { threshold: 0.08, rootMargin: "0px 0px -24px 0px" }
    );

    const observeAll = () => {
      const nodes = root.querySelectorAll(".reveal:not(.reveal--visible)");
      nodes.forEach((n) => {
        const rect = n.getBoundingClientRect();
        if (rect.top < window.innerHeight * 0.92 && rect.bottom > 0) {
          n.classList.add("reveal--visible");
          return;
        }
        io.observe(n);
      });
    };

    observeAll();

    const mo = new MutationObserver(() => observeAll());
    mo.observe(root, { childList: true, subtree: true });

    return () => {
      io.disconnect();
      mo.disconnect();
    };
  }, []);

  return ref;
}
