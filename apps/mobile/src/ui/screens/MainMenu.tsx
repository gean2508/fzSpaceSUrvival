import { useEffect, useState } from "react";
import { StyleSheet, Text, TouchableOpacity, View } from "react-native";
import { getClass } from "@hunger-arena/shared";
import { MenuLayout, PrimaryButton } from "./MenuLayout";
import { useGameStore } from "../../store/useGameStore";
import { isLoggedIn, logout } from "../../net/auth";
import { COLORS } from "../../theme/colors";

const MODE_LABEL: Record<string, string> = {
  offline: "Offline",
  lan: "LAN",
  online: "Online",
};

export function MainMenu() {
  const go = useGameStore((s) => s.go);
  const mode = useGameStore((s) => s.mode);
  const classId = useGameStore((s) => s.classId);
  const profile = useGameStore((s) => s.profile);
  const setProfile = useGameStore((s) => s.setProfile);
  const cls = getClass(classId);
  const [loggedIn, setLoggedIn] = useState(false);

  useEffect(() => {
    isLoggedIn().then(setLoggedIn);
  }, [profile]);

  return (
    <MenuLayout>
      <View style={styles.wrap}>
        <Text style={styles.logo}>Hunger Arena</Text>
        <Text style={styles.sub}>supervivencia · oleadas · multijugador</Text>

        <View style={styles.card}>
          <View style={styles.cardRow}>
            <Text style={styles.cardName}>
              {profile?.displayName ?? "Invitado"}
              {profile?.hasAccount ? " ✓" : ""}
            </Text>
            <Text style={styles.cardMeta}>
              {profile ? `XP ${profile.xp} · 🪙 ${profile.coins}` : "—"}
            </Text>
          </View>
          <Text style={styles.cardSub}>
            Mejor oleada: {profile?.bestWave ?? 0} · Partidas: {profile?.matches ?? 0}
          </Text>
        </View>

        <PrimaryButton style={styles.play} onPress={() => go("game")}>
          ▶ Jugar
        </PrimaryButton>

        <View style={styles.grid}>
          <MenuChip label="Modo" value={MODE_LABEL[mode]} onPress={() => go("mode")} />
          <MenuChip label="Clase" value={cls.name} onPress={() => go("class")} />
          <MenuChip
            label="Cuenta"
            value={loggedIn ? "Conectado" : "Invitado"}
            onPress={() => go("auth")}
          />
          <MenuChip label="Ajustes" value="⚙" onPress={() => go("settings")} />
        </View>

        {loggedIn && (
          <TouchableOpacity
            onPress={async () => {
              await logout();
              setProfile(null);
              setLoggedIn(false);
              go("menu");
            }}
          >
            <Text style={styles.logout}>Cerrar sesión</Text>
          </TouchableOpacity>
        )}
      </View>
    </MenuLayout>
  );
}

function MenuChip({
  label,
  value,
  onPress,
}: {
  label: string;
  value: string;
  onPress: () => void;
}) {
  return (
    <TouchableOpacity style={styles.chip} onPress={onPress} activeOpacity={0.85}>
      <Text style={styles.chipLabel}>{label}</Text>
      <Text style={styles.chipValue}>{value}</Text>
    </TouchableOpacity>
  );
}

const styles = StyleSheet.create({
  wrap: { width: "100%", maxWidth: 420, alignItems: "center", gap: 16 },
  logo: { fontSize: 40, fontWeight: "900", color: COLORS.night },
  sub: { fontSize: 13, fontWeight: "600", color: "rgba(43,47,58,0.7)" },
  card: {
    width: "100%",
    borderRadius: 16,
    backgroundColor: "rgba(255,247,230,0.88)",
    padding: 16,
  },
  cardRow: { flexDirection: "row", justifyContent: "space-between", alignItems: "center" },
  cardName: { fontWeight: "700", color: COLORS.bark },
  cardMeta: { fontSize: 12, color: "rgba(90,70,50,0.7)" },
  cardSub: { marginTop: 4, fontSize: 12, color: "rgba(90,70,50,0.7)" },
  play: { width: "100%" },
  grid: {
    width: "100%",
    flexDirection: "row",
    flexWrap: "wrap",
    gap: 12,
    justifyContent: "space-between",
  },
  chip: {
    width: "47%",
    borderRadius: 12,
    backgroundColor: "rgba(255,255,255,0.75)",
    padding: 12,
  },
  chipLabel: {
    fontSize: 10,
    fontWeight: "700",
    textTransform: "uppercase",
    color: "rgba(90,70,50,0.6)",
  },
  chipValue: { fontWeight: "700", color: COLORS.bark, marginTop: 2 },
  logout: {
    fontSize: 12,
    fontWeight: "600",
    color: "rgba(43,47,58,0.6)",
    textDecorationLine: "underline",
  },
});
