import { Schema, type } from "@colyseus/schema";

/** Static climbable/blocking box. Synced once at room create (never mutates). */
export class ObstacleState extends Schema {
  @type("string") id = "";
  @type("number") x = 0;
  @type("number") z = 0;
  @type("number") halfW = 1;
  @type("number") halfD = 1;
  @type("number") height = 1;
}

/** Gatherable node. `looted` flips authoritatively when a player gathers it. */
export class InteractableState extends Schema {
  @type("string") id = "";
  @type("string") kind = "tree";
  @type("number") x = 0;
  @type("number") z = 0;
  @type("number") radius = 1.4;
  @type("boolean") looted = false;
}
