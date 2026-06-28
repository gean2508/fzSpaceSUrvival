import { useState } from "react";
import { LANDING_COPY } from "./theme";
import {
  formatFileSize,
  useDownloadOptions,
  type DownloadOption,
} from "./useDownloads";
import { playLandingClick, toggleLandingMute } from "./useLandingAudio";
import { triggerFileDownload, downloadErrorMessage } from "./downloadArtifact";

const FEATURES = [
  {
    icon: "🚀",
    title: "MUNDO NATIVO",
    desc: "Motor Bevy ECS. Isla procedural, máquinas, craft.",
  },
  {
    icon: "📼",
    title: "MODO OFFLINE",
    desc: "Sim determinista. Progreso local sin uplink.",
  },
  {
    icon: "📡",
    title: "LAN / ONLINE",
    desc: "WiFi o servidor remoto cuando esté activo.",
  },
  {
    icon: "🛠",
    title: "SUPERVIVENCIA FP",
    desc: "Exploración, recursos, base — sin battle royale.",
  },
] as const;

const STEPS = [
  { n: "1", text: "Descargá el APK en tu Android (misma WiFi que el servidor para LAN)." },
  { n: "2", text: "Permití “Instalar apps desconocidas” si Android lo solicita." },
  { n: "3", text: "Elegí modo Offline en la app para jugar sin conexión." },
] as const;

const FAQ = [
  {
    id: "offline",
    q: "¿Puedo jugar sin internet?",
    a: "Sí. El modo offline usa la misma simulación que el servidor. Tu progreso se guarda localmente.",
  },
  {
    id: "lan",
    q: "¿Cómo juego en LAN?",
    a: "Levantá npm run dev:server en la PC y conectá el celular a la misma WiFi. La app ya viene con la IP de tu red.",
  },
  {
    id: "update",
    q: "¿Se actualiza sola?",
    a: "Al abrir, busca builds nuevas en el servidor, descarga el APK e instala automáticamente.",
  },
] as const;

function scrollTo(id: string) {
  document.getElementById(id)?.scrollIntoView({ behavior: "smooth", block: "start" });
}

function DownloadButton({ opt }: { opt: DownloadOption }) {
  const size = formatFileSize(opt.sizeBytes);
  const [err, setErr] = useState<string | null>(null);

  if (opt.status === "ready" && opt.url && opt.filename) {
    return (
      <div>
        <button
          type="button"
          className="landing__dl-btn landing__dl-btn--ready"
          onClick={() => {
            playLandingClick();
            setErr(null);
            try {
              triggerFileDownload(opt.url!, opt.filename!);
            } catch (e) {
              setErr(downloadErrorMessage(e));
            }
          }}
        >
          <span className="landing__dl-icon">{opt.icon}</span>
          <span className="landing__dl-text">
            <strong>{opt.label}</strong>
            <small>
              {opt.version ? `v${opt.version}` : "Listo"}
              {size ? ` · ${size}` : ""}
            </small>
          </span>
          <span className="landing__dl-arrow">⬇</span>
        </button>
        {err && <p className="landing__warn">{err}</p>}
      </div>
    );
  }

  return (
    <div
      className={`landing__dl-btn landing__dl-btn--${opt.status}`}
      aria-disabled
      title={opt.notes ?? undefined}
    >
      <span className="landing__dl-icon">{opt.icon}</span>
      <span className="landing__dl-text">
        <strong>{opt.label}</strong>
        <small>{opt.status === "soon" ? "Próximamente" : opt.notes ?? "No publicado"}</small>
      </span>
    </div>
  );
}

export function LandingPage() {
  const { options, loading, error, serverOnline, serverUrl } = useDownloadOptions();
  const [faqOpen, setFaqOpen] = useState<string | null>(null);
  const [muted, setMuted] = useState(false);
  const android = options.find((o) => o.platform === "android");

  return (
    <div className="landing__content">
      <nav className="landing__nav">
        <button type="button" className="landing__nav-brand" onClick={() => scrollTo("hero")}>
          <span>Star</span>
          <span className="landing__nav-accent">ship</span>
        </button>
        <div className="landing__nav-links">
          <button type="button" onClick={() => scrollTo("features")}>
            BRIEFING
          </button>
          <button type="button" onClick={() => scrollTo("download")}>
            DEPLOY
          </button>
          <button type="button" onClick={() => scrollTo("faq")}>
            INTEL
          </button>
          <button
            type="button"
            onClick={() => {
              playLandingClick();
              setMuted(toggleLandingMute());
            }}
            title="Música synth"
          >
            {muted ? "🔇" : "🔊"}
          </button>
        </div>
      </nav>

      <section id="hero" className="landing__hero-section">
        <div className="landing__hero">
          <p className="landing__eyebrow">{LANDING_COPY.landingEyebrow}</p>
          <h1 className="landing__title">
            <span>HUNGER</span>
            <span className="landing__title-accent">ARENA</span>
          </h1>
          <p className="landing__tagline">{LANDING_COPY.landingTagline}</p>
          <div className="landing__hero-actions">
            {android?.available && android.url && android.filename ? (
              <button
                type="button"
                className="landing__cta"
                onClick={() => {
                  playLandingClick();
                  void triggerFileDownload(android.url!, android.filename!);
                }}
              >
                {LANDING_COPY.landingCta}
              </button>
            ) : (
              <button type="button" className="landing__cta" onClick={() => { playLandingClick(); scrollTo("download"); }}>
                VER DEPLOY
              </button>
            )}
            <button type="button" className="landing__cta landing__cta--ghost" onClick={() => { playLandingClick(); scrollTo("features"); }}>
              {LANDING_COPY.landingExplore}
            </button>
          </div>
          <p className="landing__live-badge">▶ BLOOD DRAGON EDITION · VHS SURVIVAL OPS</p>
        </div>
        <div className="landing__scroll-hint" aria-hidden>
          <span>scroll</span>
          <span className="landing__scroll-chevron">⌄</span>
        </div>
      </section>

      <section id="features" className="landing__section">
        <h2 className="landing__section-title reveal">BRIEFING TÁCTICO</h2>
        <p className="landing__section-lead reveal">
          Supervivencia real con IA hostil. Mismo motor en móvil, desktop y servidor.
        </p>
        <div className="landing__features">
          {FEATURES.map((f) => (
            <article key={f.title} className="landing__feature reveal">
              <span className="landing__feature-icon">{f.icon}</span>
              <h3>{f.title}</h3>
              <p>{f.desc}</p>
            </article>
          ))}
        </div>
      </section>

      <section id="download" className="landing__section">
        <div className="landing__card reveal">
          <div className="landing__card-head">
            <h2 className="landing__card-title">DEPLOY APK</h2>
            {serverOnline !== null && (
              <span
                className={`landing__server-badge landing__server-badge--${serverOnline ? "on" : "off"}`}
                title={serverUrl}
              >
                {serverOnline ? "Servidor online" : "Servidor offline — APK estático OK"}
              </span>
            )}
          </div>
          <p className="landing__lead">
            Builds publicadas en el servidor o empaquetadas en esta página. Funciona en LAN desde el celular.
          </p>

          {loading && <p className="landing__muted">Buscando builds disponibles…</p>}

          <div className="landing__downloads">
            {options.map((opt) => (
              <DownloadButton key={opt.platform} opt={opt} />
            ))}
          </div>

          {error && !android?.available && (
            <p className="landing__warn">
              Manifiesto no disponible — probamos APK estático en /downloads/.
            </p>
          )}

          <ol className="landing__steps">
            {STEPS.map((s) => (
              <li key={s.n}>
                <span className="landing__step-n">{s.n}</span>
                {s.text}
              </li>
            ))}
          </ol>
        </div>
      </section>

      <section id="faq" className="landing__section landing__section--last">
        <div className="landing__card landing__faq reveal">
          <h2 className="landing__card-title">Preguntas</h2>
          {FAQ.map((item) => (
            <div key={item.id}>
              <button
                type="button"
                className="landing__faq-item"
                aria-expanded={faqOpen === item.id}
                onClick={() => setFaqOpen((c) => (c === item.id ? null : item.id))}
              >
                <span>{item.q}</span>
                <span className="landing__faq-chevron">{faqOpen === item.id ? "−" : "+"}</span>
              </button>
              {faqOpen === item.id && <p className="landing__faq-answer">{item.a}</p>}
            </div>
          ))}
        </div>
      </section>

      <footer className="landing__footer reveal">
        <p>Starship · Agencia Espacial Survival</p>
        <p className="landing__footer-meta">{serverUrl}</p>
      </footer>
    </div>
  );
}
