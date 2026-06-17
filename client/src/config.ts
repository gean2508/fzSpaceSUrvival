/**
 * Server URL — never hardcode your laptop IP.
 *
 * Set before build/run:
 *   VITE_SERVER_HOST=192.168.1.42 VITE_SERVER_PORT=2567 npm run dev
 *
 * Or edit .env.local (see .env.example).
 */
const host =
  import.meta.env.VITE_SERVER_HOST ||
  (typeof window !== "undefined" ? window.location.hostname : "localhost");

const port = import.meta.env.VITE_SERVER_PORT || "2567";

const wsProto =
  typeof window !== "undefined" && window.location.protocol === "https:"
    ? "wss"
    : "ws";
const httpProto = wsProto === "wss" ? "https" : "http";

export const SERVER_URL = `${wsProto}://${host}:${port}`;
export const SERVER_HTTP_URL = `${httpProto}://${host}:${port}`;

export const MOVE_SPEED = 8;

export const isMobile =
  typeof window !== "undefined" &&
  (("ontouchstart" in window) || navigator.maxTouchPoints > 0);
