import { Schema, type } from "@colyseus/schema";

/**
 * Networked entity (player, npc or projectile). Mirrors EntitySnapshot from the
 * sim. Import ONLY from @hunger-arena/shared — never redefine in client/server.
 */
export class EntityState extends Schema {
  @type("string") id = "";
  @type("string") kind = "player";
  @type("string") classId = "scout";
  @type("number") x = 0;
  @type("number") y = 0;
  @type("number") z = 0;
  @type("number") vx = 0;
  @type("number") vz = 0;
  @type("number") rot = 0;
  @type("number") hp = 100;
  @type("number") maxHp = 100;
  @type("boolean") dead = false;
  @type("number") team = 0;
  @type("string") ownerId = "";
  // parkour / survival
  @type("string") state = "idle";
  @type("number") stamina = 100;
  @type("number") maxStamina = 100;
  @type("number") hunger = 100;
  @type("number") thirst = 100;
  /** Last input seq the server processed for this player (reconciliation). */
  @type("number") lastSeq = 0;
}
