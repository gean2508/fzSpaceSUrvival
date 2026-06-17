import { ScrollView, StyleSheet, Text, TouchableOpacity, View } from "react-native";
import { CLASSES, getAbility } from "@hunger-arena/shared";
import { MenuLayout } from "./MenuLayout";
import { useGameStore } from "../../store/useGameStore";
import { COLORS } from "../../theme/colors";

const CLASS_ICON: Record<string, string> = {
  scout: "🏹",
  warrior: "🛡️",
  mage: "🔥",
};

export function ClassSelect() {
  const go = useGameStore((s) => s.go);
  const classId = useGameStore((s) => s.classId);
  const setClassId = useGameStore((s) => s.setClassId);

  return (
    <MenuLayout title="Elige clase" onBack={() => go("menu")}>
      <ScrollView horizontal showsHorizontalScrollIndicator={false} contentContainerStyle={styles.row}>
        {Object.values(CLASSES).map((c) => (
          <TouchableOpacity
            key={c.id}
            style={[styles.card, classId === c.id && styles.cardActive]}
            onPress={() => {
              setClassId(c.id);
              go("menu");
            }}
          >
            <Text style={styles.icon}>{CLASS_ICON[c.id] ?? "⚔️"}</Text>
            <Text style={[styles.name, classId === c.id && styles.nameActive]}>{c.name}</Text>
            <Text style={[styles.stats, classId === c.id && styles.statsActive]}>
              ❤ {c.maxHp} · 👟 {c.moveSpeed}
            </Text>
            {c.abilities.map((aid) => {
              const ab = getAbility(aid);
              return (
                <Text key={aid} style={[styles.ab, classId === c.id && styles.abActive]}>
                  • {ab?.name ?? aid}
                </Text>
              );
            })}
          </TouchableOpacity>
        ))}
      </ScrollView>
    </MenuLayout>
  );
}

const styles = StyleSheet.create({
  row: { gap: 12, paddingHorizontal: 8 },
  card: {
    width: 160,
    borderRadius: 16,
    padding: 16,
    backgroundColor: "rgba(255,247,230,0.9)",
    alignItems: "center",
  },
  cardActive: { backgroundColor: COLORS.sunset },
  icon: { fontSize: 36 },
  name: { fontSize: 18, fontWeight: "800", color: COLORS.bark, marginTop: 8 },
  nameActive: { color: COLORS.white },
  stats: { fontSize: 12, color: "rgba(90,70,50,0.8)", marginTop: 4 },
  statsActive: { color: "rgba(255,255,255,0.9)" },
  ab: { fontSize: 11, color: "rgba(90,70,50,0.85)", marginTop: 2 },
  abActive: { color: "rgba(255,255,255,0.9)" },
});
