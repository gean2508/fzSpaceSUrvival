import { useState } from "react";
import { MenuLayout, PrimaryButton } from "./MenuLayout";
import { useGameStore } from "../../store/useGameStore";
import { login, register } from "../../net/auth";

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
      <div className="float-in flex w-full max-w-sm flex-col gap-4 rounded-2xl bg-[var(--color-cream)]/90 p-6 text-[var(--color-bark)] shadow-xl">
        <div className="flex overflow-hidden rounded-xl bg-black/10 p-1 text-sm font-bold">
          {(["login", "register"] as const).map((t) => (
            <button
              key={t}
              className={
                "flex-1 rounded-lg py-2 transition " +
                (tab === t ? "bg-[var(--color-sunset)] text-white" : "")
              }
              onClick={() => setTab(t)}
            >
              {t === "login" ? "Entrar" : "Crear cuenta"}
            </button>
          ))}
        </div>

        {tab === "register" && (
          <Field
            label="Nombre"
            value={displayName}
            onChange={setDisplayName}
            placeholder="Tu apodo"
          />
        )}
        <Field label="Email" value={email} onChange={setEmail} placeholder="tu@email.com" type="email" />
        <Field
          label="Contraseña"
          value={password}
          onChange={setPassword}
          placeholder="mínimo 6 caracteres"
          type="password"
        />

        {error && <p className="text-sm font-semibold text-red-600">{error}</p>}

        <PrimaryButton className="w-full" onClick={busy ? undefined : submit}>
          {busy ? "…" : tab === "login" ? "Entrar" : "Crear cuenta"}
        </PrimaryButton>
        <p className="text-center text-[11px] opacity-70">
          Tu progreso de invitado se vincula automáticamente al crear cuenta.
        </p>
      </div>
    </MenuLayout>
  );
}

function Field({
  label,
  value,
  onChange,
  placeholder,
  type = "text",
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  type?: string;
}) {
  return (
    <label className="flex flex-col gap-1 text-sm font-bold">
      {label}
      <input
        type={type}
        value={value}
        placeholder={placeholder}
        onChange={(e) => onChange(e.target.value)}
        className="rounded-xl border border-black/15 bg-white px-3 py-2 font-normal outline-none focus:border-[var(--color-sunset)]"
      />
    </label>
  );
}
