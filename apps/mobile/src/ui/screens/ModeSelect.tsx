import { StyleSheet, Text, TouchableOpacity, View } from "react-native";
import { MenuLayout } from "./MenuLayout";
import { useGameStore } from "../../store/useGameStore";
import type { GameMode } from "@hunger-arena/client-core";
import { COLORS } from "../../theme/colors";

const MODES: { id: GameMode; name: string; desc: string; icon: string }[] = [
  { id: "offline", name: "Offline", desc: "Juega sin conexión. Tu progreso se guarda y sincroniza luego.", icon: "🌙" },
  { id: "lan", name: "LAN", desc: "Conéctate al servidor de tu red local (misma WiFi).", icon: "🏠" },
  { id: "online", name: "Online", desc: "Juega con cualquiera en el servidor remoto.", icon: "🌍" },
];

export function ModeSelect() {
  const go = useGameStore((s) => s.go);
  const mode = useGameStore((s) => s.mode);
  const setMode = useGameStore((s) => s.setMode);

  return (
    <MenuLayout title="Elige modo" onBack={() => go("menu")}>
      <View style={styles.list}>
        {MODES.map((m) => (
          <TouchableOpacity
            key={m.id}
            style={[styles.item, mode === m.id && styles.itemActive]}
            onPress={() => {
              setMode(m.id);
              go("menu");
            }}
          >
            <Text style={styles.icon}>{m.icon}</Text>
            <View style={styles.text}>
              <Text style={[styles.name, mode === m.id && styles.nameActive]}>{m.name}</Text>
              <Text style={[styles.desc, mode === m.id && styles.descActive]}>{m.desc}</Text>
            </View>
          </TouchableOpacity>
        ))}
      </View>
    </MenuLayout>
  );
}

const styles = StyleSheet.create({
  list: { width: "100%", maxWidth: 420, gap: 12 },
  item: {
    flexDirection: "row",
    alignItems: "center",
    gap: 14,
    borderRadius: 16,
    padding: 16,
    backgroundColor: "rgba(255,247,230,0.9)",
  },
  itemActive: { backgroundColor: COLORS.sunset },
  icon: { fontSize: 28 },
  text: { flex: 1 },
  name: { fontSize: 18, fontWeight: "800", color: COLORS.bark },
  nameActive: { color: COLORS.white },
  desc: { fontSize: 12, color: "rgba(90,70,50,0.8)", marginTop: 2 },
  descActive: { color: "rgba(255,255,255,0.9)" },
});
