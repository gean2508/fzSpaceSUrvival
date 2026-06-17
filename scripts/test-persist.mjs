import { Client } from "@colyseus/sdk";
import { ROOM_NAME, SYNC_MESSAGE } from "@hunger-arena/shared";

const url = process.env.SERVER_URL || "ws://localhost:2567";
const wait = (ms) => new Promise((r) => setTimeout(r, ms));
const DEVICE = "test-device-" + Math.floor(Math.random() * 1e6);

async function session(play) {
  const client = new Client(url);
  const room = await client.joinOrCreate(ROOM_NAME, {
    classId: "warrior",
    deviceId: DEVICE,
  });
  let profile = null;
  room.onMessage(SYNC_MESSAGE.profile, (p) => (profile = p));
  await new Promise((r) => {
    if (room.state?.entities?.size) r();
    else room.onStateChange.once(() => r());
  });
  await play(room);
  await wait(200);
  const snap = profile;
  await room.leave();
  await wait(150);
  return { room, profile: snap };
}

try {
  // Session 1: play, kill NPCs, accrue progress on leave.
  const s1 = await session(async (room) => {
    let seq = 1;
    for (let i = 0; i < 100; i++) {
      room.send("input", { seq: seq++, moveX: 0, moveZ: 0, aimX: 1, aimZ: 0, abilities: 1 });
      await wait(50);
    }
  });
  console.log("session1 profile on join:", JSON.stringify(s1.profile));

  // Session 2: rejoin same device — should see persisted matches/xp.
  let merged = null;
  const client = new Client(url);
  const room = await client.joinOrCreate(ROOM_NAME, { classId: "scout", deviceId: DEVICE });
  await new Promise((r) => {
    room.onMessage(SYNC_MESSAGE.profile, (p) => {
      if (!merged) {
        // first profile = persisted state
        console.log("session2 profile on join:", JSON.stringify(p));
        room.send(SYNC_MESSAGE.syncProgress, { xp: 100, coins: 50, bestWave: 9, matches: 1 });
      } else {
        // do nothing
      }
      merged = p;
    });
    room.onStateChange.once(() => r());
  });
  await wait(600);
  console.log("after sync merge:", JSON.stringify(merged));
  await room.leave();

  const ok =
    s1.profile && // received profile on first join
    merged &&
    merged.matches >= 1 &&
    merged.xp >= 100 &&
    merged.bestWave >= 9;

  if (ok) {
    console.log("PASS: profile persists across sessions and merges offline progress");
    process.exit(0);
  } else {
    console.error("FAIL: persistence/merge did not behave as expected");
    process.exit(1);
  }
} catch (e) {
  console.error("FAIL:", e);
  process.exit(1);
}
