import { useRef, useState } from "react";
import { Pressable, StyleSheet, Text, View } from "react-native";
import { Gesture, GestureDetector } from "react-native-gesture-handler";
import type { AbilityDef } from "@hunger-arena/shared";
import { useControlStore } from "../store/useControlStore";
import { COLORS } from "../theme/colors";

interface TouchControlsProps {
  slots: (AbilityDef | undefined)[];
  cooldownPct: number[];
  pressSlot: (slot: number, down: boolean) => void;
}

const MOVE_RADIUS = 60;

/**
 * 4-finger landscape:
 *  - Left 45%: move joystick (finger 1)
 *  - Right upper area: aim pad (finger 2)
 *  - Parkour + ability buttons (fingers 3–4), parallel while sticks held
 */
export function TouchControls({ slots, cooldownPct, pressSlot }: TouchControlsProps) {
  const setMove = useControlStore((s) => s.setMove);
  const setAim = useControlStore((s) => s.setAim);
  const setAction = useControlStore((s) => s.setAction);
  const setSprint = useControlStore((s) => s.setSprint);

  const moveOrigin = useRef({ x: 0, y: 0 });
  const aimOrigin = useRef({ x: 0, y: 0 });
  const [moveBase, setMoveBase] = useState<{ x: number; y: number } | null>(null);
  const [moveKnob, setMoveKnob] = useState({ x: 0, y: 0 });

  const moveGesture = Gesture.Pan()
    .maxPointers(1)
    .runOnJS(true)
    .onBegin((e) => {
      moveOrigin.current = { x: e.absoluteX, y: e.absoluteY };
      setMoveBase({ x: e.absoluteX, y: e.absoluteY });
      setMoveKnob({ x: 0, y: 0 });
    })
    .onUpdate((e) => {
      const dx = e.absoluteX - moveOrigin.current.x;
      const dy = e.absoluteY - moveOrigin.current.y;
      const dist = Math.min(Math.hypot(dx, dy), MOVE_RADIUS);
      const ang = Math.atan2(dy, dx);
      setMoveKnob({ x: Math.cos(ang) * dist, y: Math.sin(ang) * dist });
      setMove(Math.cos(ang) * (dist / MOVE_RADIUS), Math.sin(ang) * (dist / MOVE_RADIUS));
      setSprint(dist > MOVE_RADIUS * 0.82);
    })
    .onFinalize(() => {
      setMoveBase(null);
      setMove(0, 0);
      setSprint(false);
    });

  const aimGesture = Gesture.Pan()
    .maxPointers(1)
    .runOnJS(true)
    .onBegin((e) => {
      aimOrigin.current = { x: e.absoluteX, y: e.absoluteY };
    })
    .onUpdate((e) => {
      const dx = e.absoluteX - aimOrigin.current.x;
      const dy = e.absoluteY - aimOrigin.current.y;
      if (Math.hypot(dx, dy) > 6) setAim(dx, dy);
    });

  return (
    <View style={styles.overlay} pointerEvents="box-none">
      <GestureDetector gesture={moveGesture}>
        <View style={styles.moveZone} />
      </GestureDetector>

      <GestureDetector gesture={aimGesture}>
        <View style={styles.aimZone} />
      </GestureDetector>

      {moveBase && (
        <View
          pointerEvents="none"
          style={[
            styles.joystickBase,
            { left: moveBase.x - MOVE_RADIUS, top: moveBase.y - MOVE_RADIUS },
          ]}
        >
          <View
            style={[
              styles.joystickKnob,
              { transform: [{ translateX: moveKnob.x }, { translateY: moveKnob.y }] },
            ]}
          />
        </View>
      )}

      <Pressable
        style={styles.parkourBtn}
        onPressIn={() => setAction(true)}
        onPressOut={() => setAction(false)}
      >
        <Text style={styles.parkourText}>PARKOUR</Text>
      </Pressable>

      <View style={styles.abilityRow} pointerEvents="box-none">
        {slots.map((ab, i) =>
          ab ? (
            <Pressable
              key={i}
              style={styles.abilityBtn}
              onPressIn={() => pressSlot(i, true)}
              onPressOut={() => pressSlot(i, false)}
            >
              <Text style={styles.abilityText}>{ab.name.slice(0, 4)}</Text>
              {cooldownPct[i] > 0 && (
                <View
                  style={[styles.cooldownFill, { height: `${cooldownPct[i] * 100}%` }]}
                />
              )}
            </Pressable>
          ) : null
        )}
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  overlay: { ...StyleSheet.absoluteFill, zIndex: 10 },
  moveZone: {
    position: "absolute",
    left: 0,
    top: 0,
    width: "45%",
    height: "100%",
  },
  /** Upper-right only — leaves room for parkour/ability buttons */
  aimZone: {
    position: "absolute",
    right: 0,
    top: 0,
    width: "55%",
    height: "62%",
  },
  joystickBase: {
    position: "absolute",
    width: MOVE_RADIUS * 2,
    height: MOVE_RADIUS * 2,
    borderRadius: MOVE_RADIUS,
    backgroundColor: "rgba(94,194,214,0.12)",
    borderWidth: 2,
    borderColor: "rgba(94,194,214,0.3)",
    zIndex: 5,
  },
  joystickKnob: {
    position: "absolute",
    left: MOVE_RADIUS - 26,
    top: MOVE_RADIUS - 26,
    width: 52,
    height: 52,
    borderRadius: 26,
    backgroundColor: "rgba(94,194,214,0.5)",
  },
  parkourBtn: {
    position: "absolute",
    right: 150,
    bottom: 40,
    width: 84,
    height: 84,
    borderRadius: 42,
    backgroundColor: "rgba(244,162,89,0.85)",
    alignItems: "center",
    justifyContent: "center",
    zIndex: 20,
  },
  parkourText: { fontWeight: "800", fontSize: 13, color: COLORS.night },
  abilityRow: {
    position: "absolute",
    right: 24,
    bottom: 130,
    flexDirection: "row",
    gap: 12,
    zIndex: 20,
  },
  abilityBtn: {
    width: 62,
    height: 62,
    borderRadius: 31,
    backgroundColor: "rgba(43,47,58,0.8)",
    borderWidth: 2,
    borderColor: "rgba(255,247,230,0.3)",
    alignItems: "center",
    justifyContent: "center",
    overflow: "hidden",
  },
  abilityText: { color: COLORS.cream, fontWeight: "700", fontSize: 11 },
  cooldownFill: {
    position: "absolute",
    left: 0,
    bottom: 0,
    width: "100%",
    backgroundColor: "rgba(0,0,0,0.55)",
  },
});
