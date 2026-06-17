import { Component, type ErrorInfo, type ReactNode } from "react";
import { clog } from "./DebugOverlay";

interface Props {
  children: ReactNode;
}
interface State {
  error: Error | null;
}

/**
 * Top-level React error boundary. Prevents the "black screen of death" by
 * rendering a readable crash card and logging the error (rule 9 observability).
 */
export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    clog("crash", error.message, { stack: info.componentStack?.slice(0, 400) });
  }

  render() {
    const { error } = this.state;
    if (!error) return this.props.children;
    return (
      <div
        style={{
          position: "fixed",
          inset: 0,
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          justifyContent: "center",
          gap: 16,
          background: "#2b2f3a",
          color: "#fff7e6",
          fontFamily: "monospace",
          padding: 24,
          textAlign: "center",
        }}
      >
        <div style={{ fontSize: 40 }}>💥</div>
        <h1 style={{ fontSize: 20, fontWeight: 800 }}>Algo se rompió</h1>
        <pre
          style={{
            maxWidth: 560,
            whiteSpace: "pre-wrap",
            fontSize: 12,
            opacity: 0.85,
          }}
        >
          {error.message}
        </pre>
        <button
          onClick={() => window.location.reload()}
          style={{
            background: "#f4a259",
            color: "#2b2f3a",
            border: "none",
            borderRadius: 12,
            padding: "12px 24px",
            fontWeight: 700,
            cursor: "pointer",
          }}
        >
          Recargar
        </button>
      </div>
    );
  }
}

/** Capture uncaught errors / promise rejections into the client log ring. */
export function installGlobalErrorCapture() {
  if (typeof window === "undefined") return;
  window.addEventListener("error", (e) => {
    clog("error", e.message, { src: e.filename, line: e.lineno });
  });
  window.addEventListener("unhandledrejection", (e) => {
    const reason = e.reason;
    clog("reject", reason instanceof Error ? reason.message : String(reason));
  });
}
