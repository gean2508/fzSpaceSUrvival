import { useEffect, useRef, type RefObject } from "react";

/**
 * Desktop aim: Pointer Lock + movementX for top-down facing.
 * Falls back to scene AimPlane raycast when lock is not active.
 */
export function useDesktopAim(
  containerRef: RefObject<HTMLElement | null>,
  setAim: (x: number, z: number) => void,
  active: boolean
) {
  const angleRef = useRef(Math.PI / 2);

  useEffect(() => {
    if (!active) return;
    const el = containerRef.current;
    if (!el) return;

    const requestLock = () => {
      if (document.pointerLockElement !== el) {
        void el.requestPointerLock();
      }
    };
    el.addEventListener("click", requestLock);
    return () => el.removeEventListener("click", requestLock);
  }, [containerRef, active]);

  useEffect(() => {
    if (!active) return;

    const onMove = (e: MouseEvent) => {
      if (document.pointerLockElement !== containerRef.current) return;
      angleRef.current -= e.movementX * 0.004;
      const x = Math.sin(angleRef.current);
      const z = Math.cos(angleRef.current);
      setAim(x, z);
    };

    document.addEventListener("mousemove", onMove);
    return () => document.removeEventListener("mousemove", onMove);
  }, [containerRef, active, setAim]);
}
