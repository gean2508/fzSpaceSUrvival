/**
 * Desktop (Tauri) server URL — never hardcode your laptop IP.
 *
 *   VITE_SERVER_HOST=192.168.1.42 VITE_SERVER_PORT=2567 npm run dev:tauri
 */
const host = import.meta.env.VITE_SERVER_HOST ?? "localhost";
const port = import.meta.env.VITE_SERVER_PORT ?? "2567";

export const SERVER_URL = `ws://${host}:${port}`;
export const SERVER_HTTP_URL = `http://${host}:${port}`;

export const MOVE_SPEED = 8;

/** Tauri desktop is always keyboard + mouse. */
export const isMobile = false;
