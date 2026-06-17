import { useEffect } from "react";
import { StyleSheet, Text, TouchableOpacity } from "react-native";
import { MenuLayout } from "./MenuLayout";
import { useGameStore } from "../../store/useGameStore";
import { COLORS } from "../../theme/colors";

export function SplashScreen() {
  const go = useGameStore((s) => s.go);

  useEffect(() => {
    const t = setTimeout(() => go("menu"), 1800);
    return () => clearTimeout(t);
  }, [go]);

  return (
    <MenuLayout>
      <TouchableOpacity style={styles.wrap} onPress={() => go("menu")} activeOpacity={0.9}>
        <Text style={styles.hunger}>Hunger</Text>
        <Text style={styles.arena}>Arena</Text>
        <Text style={styles.hint}>toca para empezar</Text>
      </TouchableOpacity>
    </MenuLayout>
  );
}

const styles = StyleSheet.create({
  wrap: { alignItems: "center" },
  hunger: {
    fontSize: 56,
    fontWeight: "900",
    color: COLORS.night,
  },
  arena: {
    fontSize: 56,
    fontWeight: "900",
    color: COLORS.sunset,
    marginTop: -8,
  },
  hint: {
    marginTop: 16,
    fontSize: 14,
    fontWeight: "600",
    color: "rgba(43,47,58,0.7)",
  },
});
