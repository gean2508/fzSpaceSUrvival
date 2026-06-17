/**
 * Server URL — never hardcode your laptop IP.
 *
 * Set before build/run:
 *   EXPO_PUBLIC_SERVER_HOST=192.168.1.42 EXPO_PUBLIC_SERVER_PORT=2567 npx expo start
 */
const host = process.env.EXPO_PUBLIC_SERVER_HOST ?? "localhost";
const port = process.env.EXPO_PUBLIC_SERVER_PORT ?? "2567";

export const SERVER_URL = `ws://${host}:${port}`;
export const SERVER_HTTP_URL = `http://${host}:${port}`;

export const MOVE_SPEED = 8;

/** React Native build is always touch-first. */
export const isMobile = true;
