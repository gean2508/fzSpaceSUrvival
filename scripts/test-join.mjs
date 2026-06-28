import { Client } from "@colyseus/sdk";
import { ROOM_NAME } from "@hunger-arena/shared";
import { makeInput } from "./e2e-input.mjs";

const url = process.env.SERVER_URL || "ws://localhost:2567";

const wait = (ms) => new Promise((r) => setTimeout(r, ms));

try {
  const client = new Client(url);
  const room = await client.joinOrCreate(ROOM_NAME, { classId: "scout" });

  await new Promise((resolve) => {
    if (room.state?.entities?.size) resolve();
    else room.onStateChange.once(() => resolve());
  });

  const me = () => room.state.entities.get(room.sessionId);
  const before = me();
  const startX = before?.x ?? 0;
  console.log("OK joined:", room.sessionId, "entities:", room.state.entities.size, "x0:", startX.toFixed(3));

  // Send authoritative input (move +X) for ~1s.
  for (let seq = 1; seq <= 25; seq++) {
    room.send("input", makeInput(seq, { moveX: 1, aimX: 0, aimZ: 0 }));
    await wait(40);
  }
  await wait(150);

  const after = me();
  const moved = (after?.x ?? 0) - startX;
  console.log("after x:", after?.x?.toFixed(3), "delta:", moved.toFixed(3), "lastSeq:", after?.lastSeq);

  await room.leave();
  if (moved > 0.5) {
    console.log("PASS: server moved player from validated input only");
    process.exit(0);
  } else {
    console.error("FAIL: player did not move as expected");
    process.exit(1);
  }
} catch (e) {
  console.error("FAIL:", e);
  process.exit(1);
}
