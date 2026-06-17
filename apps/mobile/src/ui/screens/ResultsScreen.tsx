import { StyleSheet, Text, TouchableOpacity, View } from "react-native";
import { MenuLayout, PrimaryButton } from "./MenuLayout";
import { useGameStore } from "../../store/useGameStore";
import { COLORS } from "../../theme/colors";

export function ResultsScreen() {
  const go = useGameStore((s) => s.go);
  const result = useGameStore((s) => s.lastResult);
  const profile = useGameStore((s) => s.profile);

  return (
    <MenuLayout title="Fin de la partida">
      <View style={styles.card}>
        <Text style={styles.trophy}>🏆</Text>
        <Text style={styles.label}>Oleadas superadas</Text>
        <Text style={styles.waves}>{result?.waves ?? 0}</Text>

        {profile && (
          <View style={styles.stats}>
            <Stat label="XP" value={profile.xp} />
            <Stat label="Monedas" value={profile.coins} />
            <Stat label="Mejor" value={profile.bestWave} />
          </View>
        )}

        <View style={styles.actions}>
          <PrimaryButton style={styles.btn} onPress={() => go("game")}>
            Reintentar
          </PrimaryButton>
          <TouchableOpacity style={styles.secondary} onPress={() => go("menu")}>
            <Text style={styles.secondaryText}>Menú</Text>
          </TouchableOpacity>
        </View>
      </View>
    </MenuLayout>
  );
}

function Stat({ label, value }: { label: string; value: number }) {
  return (
    <View style={styles.stat}>
      <Text style={styles.statLabel}>{label}</Text>
      <Text style={styles.statValue}>{value}</Text>
    </View>
  );
}

const styles = StyleSheet.create({
  card: {
    width: "100%",
    maxWidth: 360,
    borderRadius: 16,
    backgroundColor: "rgba(255,247,230,0.9)",
    padding: 28,
    alignItems: "center",
    gap: 8,
  },
  trophy: { fontSize: 56 },
  label: {
    fontSize: 12,
    fontWeight: "600",
    textTransform: "uppercase",
    color: "rgba(90,70,50,0.7)",
  },
  waves: { fontSize: 48, fontWeight: "900", color: COLORS.sunset },
  stats: { flexDirection: "row", gap: 8, width: "100%", marginTop: 8 },
  stat: {
    flex: 1,
    borderRadius: 12,
    backgroundColor: "rgba(255,255,255,0.7)",
    padding: 8,
    alignItems: "center",
  },
  statLabel: { fontSize: 10, fontWeight: "700", color: "rgba(90,70,50,0.6)" },
  statValue: { fontSize: 18, fontWeight: "800", color: COLORS.bark },
  actions: { flexDirection: "row", gap: 12, width: "100%", marginTop: 12 },
  btn: { flex: 1 },
  secondary: {
    flex: 1,
    borderRadius: 16,
    backgroundColor: "rgba(0,0,0,0.12)",
    alignItems: "center",
    justifyContent: "center",
    paddingVertical: 16,
  },
  secondaryText: { fontWeight: "700", color: COLORS.bark },
});
