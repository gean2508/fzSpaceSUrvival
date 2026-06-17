import { useState } from "react";
import { StyleSheet, Text, TextInput, TouchableOpacity, View } from "react-native";
import { MenuLayout, PrimaryButton } from "./MenuLayout";
import { useGameStore } from "../../store/useGameStore";
import { login, register } from "../../net/auth";
import { COLORS } from "../../theme/colors";

export function AuthScreen() {
  const go = useGameStore((s) => s.go);
  const setProfile = useGameStore((s) => s.setProfile);

  const [tab, setTab] = useState<"login" | "register">("login");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async () => {
    setBusy(true);
    setError(null);
    try {
      const profile =
        tab === "login"
          ? await login(email, password)
          : await register(email, password, displayName || undefined);
      setProfile(profile);
      go("menu");
    } catch (e) {
      setError(e instanceof Error ? e.message : "error");
    } finally {
      setBusy(false);
    }
  };

  return (
    <MenuLayout title="Cuenta" onBack={() => go("menu")}>
      <View style={styles.card}>
        <View style={styles.tabs}>
          {(["login", "register"] as const).map((t) => (
            <TouchableOpacity
              key={t}
              style={[styles.tab, tab === t && styles.tabActive]}
              onPress={() => setTab(t)}
            >
              <Text style={[styles.tabText, tab === t && styles.tabTextActive]}>
                {t === "login" ? "Entrar" : "Crear cuenta"}
              </Text>
            </TouchableOpacity>
          ))}
        </View>

        {tab === "register" && (
          <Field label="Nombre" value={displayName} onChange={setDisplayName} placeholder="Tu apodo" />
        )}
        <Field label="Email" value={email} onChange={setEmail} placeholder="tu@email.com" />
        <Field
          label="Contraseña"
          value={password}
          onChange={setPassword}
          placeholder="mínimo 6 caracteres"
          secure
        />

        {error && <Text style={styles.error}>{error}</Text>}

        <PrimaryButton style={styles.submit} onPress={busy ? undefined : submit}>
          {busy ? "…" : tab === "login" ? "Entrar" : "Crear cuenta"}
        </PrimaryButton>
        <Text style={styles.hint}>
          Tu progreso de invitado se vincula automáticamente al crear cuenta.
        </Text>
      </View>
    </MenuLayout>
  );
}

function Field({
  label,
  value,
  onChange,
  placeholder,
  secure,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  secure?: boolean;
}) {
  return (
    <View style={styles.field}>
      <Text style={styles.label}>{label}</Text>
      <TextInput
        style={styles.input}
        value={value}
        placeholder={placeholder}
        onChangeText={onChange}
        secureTextEntry={secure}
        autoCapitalize="none"
      />
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
  tabs: {
    flexDirection: "row",
    gap: 8,
    backgroundColor: "rgba(0,0,0,0.08)",
    borderRadius: 12,
    padding: 4,
  },
  tab: { flex: 1, paddingVertical: 10, borderRadius: 10, alignItems: "center" },
  tabActive: { backgroundColor: COLORS.sunset },
  tabText: { fontWeight: "700", color: COLORS.bark },
  tabTextActive: { color: COLORS.white },
  field: { gap: 4 },
  label: { fontSize: 13, fontWeight: "700", color: COLORS.bark },
  input: {
    borderRadius: 12,
    borderWidth: 1,
    borderColor: "rgba(0,0,0,0.12)",
    backgroundColor: COLORS.white,
    paddingHorizontal: 12,
    paddingVertical: 10,
    fontSize: 15,
  },
  error: { color: "#dc2626", fontWeight: "600", fontSize: 13 },
  submit: { width: "100%" },
  hint: { fontSize: 11, textAlign: "center", color: "rgba(90,70,50,0.7)" },
});
