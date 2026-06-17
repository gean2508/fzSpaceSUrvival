import { useEffect, useState } from "react";
import AsyncStorage from "@react-native-async-storage/async-storage";
import { StyleSheet, Text, TouchableOpacity, View } from "react-native";
import { MenuLayout } from "./MenuLayout";
import { useGameStore } from "../../store/useGameStore";
import { SERVER_URL } from "../../config";
import { COLORS } from "../../theme/colors";

export function SettingsScreen() {
  const go = useGameStore((s) => s.go);
  const [debug, setDebug] = useState(false);

  useEffect(() => {
    AsyncStorage.getItem("arena-debug").then((v) => setDebug(v === "1"));
  }, []);

  const toggleDebug = async () => {
    const next = !debug;
    await AsyncStorage.setItem("arena-debug", next ? "1" : "0");
    setDebug(next);
  };

  return (
    <MenuLayout title="Ajustes" onBack={() => go("menu")}>
      <View style={styles.card}>
        <Row label="Servidor">
          <Text style={styles.mono}>{SERVER_URL}</Text>
        </Row>
        <Row label="Overlay de debug">
          <TouchableOpacity
            style={[styles.toggle, debug && styles.toggleOn]}
            onPress={toggleDebug}
          >
            <Text style={styles.toggleText}>{debug ? "ON" : "OFF"}</Text>
          </TouchableOpacity>
        </Row>
        <Text style={styles.hint}>
          Para jugar en LAN, compila con EXPO_PUBLIC_SERVER_HOST=&lt;ip-laptop&gt;.
        </Text>
      </View>
    </MenuLayout>
  );
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <View style={styles.row}>
      <Text style={styles.rowLabel}>{label}</Text>
      {children}
    </View>
  );
}

const styles = StyleSheet.create({
  card: {
    width: "100%",
    maxWidth: 360,
    borderRadius: 16,
    backgroundColor: "rgba(255,247,230,0.9)",
    padding: 20,
    gap: 12,
  },
  row: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "space-between",
    borderBottomWidth: 1,
    borderBottomColor: "rgba(0,0,0,0.08)",
    paddingBottom: 10,
  },
  rowLabel: { fontSize: 14, fontWeight: "700", color: COLORS.bark },
  mono: { fontFamily: "monospace", fontSize: 11, color: COLORS.bark, maxWidth: 180 },
  toggle: {
    borderRadius: 20,
    paddingHorizontal: 14,
    paddingVertical: 6,
    backgroundColor: "rgba(0,0,0,0.25)",
  },
  toggleOn: { backgroundColor: COLORS.meadowDeep },
  toggleText: { color: COLORS.white, fontWeight: "700", fontSize: 13 },
  hint: { fontSize: 11, color: "rgba(90,70,50,0.7)", lineHeight: 16 },
});
