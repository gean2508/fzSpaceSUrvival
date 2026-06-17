import { APK_URL, APK_VERSION } from "./landing/config";

export default function App() {
  return (
    <div className="landing">
      <div className="landing__sky" />
      <div className="landing__hills" />

      <main className="landing__content">
        <header className="landing__hero">
          <h1 className="landing__title">
            <span>Hunger</span>
            <span className="landing__title-accent">Arena</span>
          </h1>
          <p className="landing__tagline">
            Supervivencia multijugador 3D · oleadas · combate por clases
          </p>
        </header>

        <section className="landing__card">
          <p className="landing__lead">
            El juego corre nativo en Android e iOS (OpenGL/Metal) con Expo + React
            Three Fiber. Esta página es el portal de descarga para instalar en tu
            celular.
          </p>

          <a className="landing__download" href={APK_URL} download>
            ⬇ Descargar APK Android
          </a>
          <p className="landing__version">Versión {APK_VERSION}</p>

          <ol className="landing__steps">
            <li>Descargá el APK en tu teléfono (misma WiFi que este servidor).</li>
            <li>Activá “Instalar apps desconocidas” si Android lo pide.</li>
            <li>Abrí Hunger Arena → elegí modo <strong>LAN</strong> para jugar contra tu laptop.</li>
          </ol>
        </section>

        <section className="landing__dev">
          <h2>Desarrollo</h2>
          <ul>
            <li>
              <code>npm run dev:server</code> — Colyseus autoritativo (:2567)
            </li>
            <li>
              <code>npm run dev:mobile</code> — app Expo (simulador o dispositivo)
            </li>
            <li>
              Compilar APK: ver README → <strong>mobile/</strong> →{" "}
              <code>npx expo prebuild</code>
            </li>
          </ul>
        </section>
      </main>
    </div>
  );
}
