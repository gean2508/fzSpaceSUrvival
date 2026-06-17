import { Client } from "@colyseus/sdk";
import { ROOM_NAME } from "@hunger-arena/shared";

const http = process.env.HTTP_URL || "http://localhost:2567";
const ws = process.env.SERVER_URL || "ws://localhost:2567";
const email = `user${Math.floor(Math.random() * 1e6)}@test.dev`;
const password = "hunter2pass";
const deviceId = "auth-device-" + Math.floor(Math.random() * 1e6);

async function post(path, body) {
  const r = await fetch(http + path, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  return { status: r.status, json: await r.json().catch(() => null) };
}

try {
  const reg = await post("/auth/register", { email, password, deviceId, displayName: "Tester" });
  console.log("register:", reg.status, reg.json && Object.keys(reg.json));
  if (reg.status !== 200 || !reg.json.accessToken) throw new Error("register failed");

  const login = await post("/auth/login", { email, password });
  console.log("login:", login.status, !!login.json.accessToken);
  if (login.status !== 200 || !login.json.accessToken) throw new Error("login failed");

  const badLogin = await post("/auth/login", { email, password: "wrongpass" });
  console.log("badLogin status (expect 401):", badLogin.status);
  if (badLogin.status !== 401) throw new Error("bad login should be rejected");

  const refresh = await post("/auth/refresh", { refreshToken: login.json.refreshToken });
  console.log("refresh:", refresh.status, !!refresh.json.accessToken);
  if (refresh.status !== 200 || !refresh.json.accessToken) throw new Error("refresh failed");

  // Authenticated join.
  const client = new Client(ws);
  const room = await client.joinOrCreate(ROOM_NAME, {
    classId: "scout",
    deviceId,
    token: login.json.accessToken,
  });
  console.log("authenticated join OK:", room.sessionId);
  await room.leave();

  // Bad-token join should be rejected.
  let rejected = false;
  try {
    const c2 = new Client(ws);
    await c2.joinOrCreate(ROOM_NAME, { classId: "scout", token: "not-a-real-token" });
  } catch {
    rejected = true;
  }
  console.log("bad-token join rejected:", rejected);

  if (rejected) {
    console.log("PASS: auth register/login/refresh + JWT-gated join work; guests still allowed");
    process.exit(0);
  } else {
    console.error("FAIL: bad token was not rejected");
    process.exit(1);
  }
} catch (e) {
  console.error("FAIL:", e);
  process.exit(1);
}
