import { useRef, useState } from "react";
import type { AbilityDef } from "@hunger-arena/shared";
import { useControlStore } from "../store/useControlStore";

interface TouchControlsProps {
  slots: (AbilityDef | undefined)[];
  cooldownPct: number[];
  pressSlot: (slot: number, down: boolean) => void;
}

const MOVE_RADIUS = 60;

/**
 * Ergonomic 4-finger landscape controls built on native PointerEvents:
 *  - Finger 1 (left half): dynamic floating joystick → movement (X/Z).
 *  - Finger 2 (right free area): floating trackpad → look/aim direction.
 *  - Fingers 3 & 4 (right buttons): parkour/action + abilities, usable in
 *    parallel while the sticks stay pressed (each tracks its own pointerId).
 *
 * Each interactive surface captures its own pointer, so all four work at once.
 */
export function TouchControls({ slots, cooldownPct, pressSlot }: TouchControlsProps) {
  const setMove = useControlStore((s) => s.setMove);
  const setAim = useControlStore((s) => s.setAim);
  const setAction = useControlStore((s) => s.setAction);
  const setSprint = useControlStore((s) => s.setSprint);

  // --- left movement joystick (dynamic, spawns where you touch) ---
  const moveId = useRef<number | null>(null);
  const moveOrigin = useRef({ x: 0, y: 0 });
  const [moveBase, setMoveBase] = useState<{ x: number; y: number } | null>(null);
  const [moveKnob, setMoveKnob] = useState({ x: 0, y: 0 });

  const onMoveDown = (e: React.PointerEvent) => {
    if (moveId.current !== null) return;
    moveId.current = e.pointerId;
    (e.target as Element).setPointerCapture(e.pointerId);
    moveOrigin.current = { x: e.clientX, y: e.clientY };
    setMoveBase({ x: e.clientX, y: e.clientY });
    setMoveKnob({ x: 0, y: 0 });
  };
  const onMoveMove = (e: React.PointerEvent) => {
    if (e.pointerId !== moveId.current) return;
    const dx = e.clientX - moveOrigin.current.x;
    const dy = e.clientY - moveOrigin.current.y;
    const dist = Math.min(Math.hypot(dx, dy), MOVE_RADIUS);
    const ang = Math.atan2(dy, dx);
    const kx = Math.cos(ang) * dist;
    const ky = Math.sin(ang) * dist;
    setMoveKnob({ x: kx, y: ky });
    setMove(kx / MOVE_RADIUS, ky / MOVE_RADIUS);
    // Push-to-edge sprint (finger 1): no extra button needed on mobile.
    setSprint(dist > MOVE_RADIUS * 0.82);
  };
  const onMoveUp = (e: React.PointerEvent) => {
    if (e.pointerId !== moveId.current) return;
    moveId.current = null;
    setMoveBase(null);
    setMove(0, 0);
    setSprint(false);
  };

  // --- right aim trackpad ---
  const aimId = useRef<number | null>(null);
  const aimOrigin = useRef({ x: 0, y: 0 });

  const onAimDown = (e: React.PointerEvent) => {
    if (aimId.current !== null) return;
    aimId.current = e.pointerId;
    (e.target as Element).setPointerCapture(e.pointerId);
    aimOrigin.current = { x: e.clientX, y: e.clientY };
  };
  const onAimMove = (e: React.PointerEvent) => {
    if (e.pointerId !== aimId.current) return;
    const dx = e.clientX - aimOrigin.current.x;
    const dy = e.clientY - aimOrigin.current.y;
    if (Math.hypot(dx, dy) > 6) setAim(dx, dy);
  };
  const onAimUp = (e: React.PointerEvent) => {
    if (e.pointerId !== aimId.current) return;
    aimId.current = null;
  };

  const stop = (e: React.PointerEvent) => e.stopPropagation();

  return (
    <div
      style={{ position: "fixed", inset: 0, zIndex: 10, touchAction: "none", userSelect: "none" }}
    >
      {/* Left movement zone */}
      <div
        onPointerDown={onMoveDown}
        onPointerMove={onMoveMove}
        onPointerUp={onMoveUp}
        onPointerCancel={onMoveUp}
        style={{ position: "absolute", left: 0, top: 0, width: "45%", height: "100%", pointerEvents: "auto" }}
      />
      {/* Right aim zone (behind buttons) */}
      <div
        onPointerDown={onAimDown}
        onPointerMove={onAimMove}
        onPointerUp={onAimUp}
        onPointerCancel={onAimUp}
        style={{ position: "absolute", right: 0, top: 0, width: "55%", height: "100%", pointerEvents: "auto" }}
      />

      {/* Dynamic move joystick visual */}
      {moveBase && (
        <div
          style={{
            position: "absolute",
            left: moveBase.x - MOVE_RADIUS,
            top: moveBase.y - MOVE_RADIUS,
            width: MOVE_RADIUS * 2,
            height: MOVE_RADIUS * 2,
            borderRadius: "50%",
            background: "rgba(94,194,214,0.12)",
            border: "2px solid rgba(94,194,214,0.3)",
            pointerEvents: "none",
          }}
        >
          <div
            style={{
              position: "absolute",
              left: MOVE_RADIUS - 26,
              top: MOVE_RADIUS - 26,
              width: 52,
              height: 52,
              borderRadius: "50%",
              background: "rgba(94,194,214,0.5)",
              transform: `translate(${moveKnob.x}px, ${moveKnob.y}px)`,
            }}
          />
        </div>
      )}

      {/* Parkour / action button (thumb-reachable, bottom-right) */}
      <button
        onPointerDown={(e) => {
          stop(e);
          (e.target as Element).setPointerCapture(e.pointerId);
          setAction(true);
        }}
        onPointerUp={(e) => {
          stop(e);
          setAction(false);
        }}
        onPointerCancel={() => setAction(false)}
        style={{
          position: "absolute",
          right: 150,
          bottom: 40,
          width: 84,
          height: 84,
          borderRadius: "50%",
          background: "rgba(244,162,89,0.85)",
          color: "#2b2f3a",
          border: "none",
          fontWeight: 800,
          fontSize: 13,
          pointerEvents: "auto",
          touchAction: "none",
        }}
      >
        PARKOUR
      </button>

      {/* Ability buttons cluster */}
      <div style={{ position: "absolute", right: 24, bottom: 130, display: "flex", gap: 12, pointerEvents: "none" }}>
        {slots.map((ab, i) =>
          ab ? (
            <button
              key={i}
              onPointerDown={(e) => {
                stop(e);
                (e.target as Element).setPointerCapture(e.pointerId);
                pressSlot(i, true);
              }}
              onPointerUp={(e) => {
                stop(e);
                pressSlot(i, false);
              }}
              onPointerCancel={() => pressSlot(i, false)}
              style={{
                position: "relative",
                width: 62,
                height: 62,
                borderRadius: "50%",
                background: "rgba(43,47,58,0.8)",
                color: "#fff7e6",
                border: "2px solid rgba(255,247,230,0.3)",
                fontWeight: 700,
                fontSize: 11,
                pointerEvents: "auto",
                touchAction: "none",
                overflow: "hidden",
              }}
            >
              {ab.name.slice(0, 4)}
              {cooldownPct[i] > 0 && (
                <span
                  style={{
                    position: "absolute",
                    left: 0,
                    bottom: 0,
                    width: "100%",
                    height: `${cooldownPct[i] * 100}%`,
                    background: "rgba(0,0,0,0.55)",
                  }}
                />
              )}
            </button>
          ) : null
        )}
      </div>
    </div>
  );
}
