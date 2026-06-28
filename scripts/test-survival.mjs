import { Client } from "@colyseus/sdk";
import { ROOM_NAME } from "@hunger-arena/shared";
import { makeInput } from "./e2e-input.mjs";

const url = process.env.SERVER_URL || "ws://localhost:2567";
const wait = (ms) => new Promise((r) => setTimeout(r, ms));

try {
  const client = new Client(url);
  const room = await client.joinOrCreate(ROOM_NAME, { classId: "warrior" });
  await new Promise((resolve) => {
    if (room.state?.entities?.size) resolve();
    else room.onStateChange.once(() => resolve());
  });

  // Wait for the first wave to spawn (~3s) and keep attacking.
  let seq = 1;
  for (let i = 0; i < 120; i++) {
    room.send("input", makeInput(seq++, { aimX: 1, abilities: 1 }));
    await wait(50);
  }

  let npcs = 0;
  let players = 0;
  room.state.entities.forEach((e) => {
    if (e.kind === "npc") npcs++;
    if (e.kind === "player") players++;
  });

  console.log("wave:", room.state.wave, "npcs:", npcs, "players:", players);
  await room.leave();

  if (room.state.wave >= 1 && npcs > 0) {
    console.log("PASS: survival waves spawned NPCs on the authoritative server");
    process.exit(0);
  } else {
    console.error("FAIL: no waves/NPCs spawned");
    process.exit(1);
  }
} catch (e) {
  console.error("FAIL:", e);
  process.exit(1);
}
