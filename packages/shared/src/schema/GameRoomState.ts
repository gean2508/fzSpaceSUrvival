import { ArraySchema, MapSchema, Schema, type } from "@colyseus/schema";
import { EntityState } from "./EntityState.js";
import { InteractableState, ObstacleState } from "./WorldSchema.js";
import { WorldPropState } from "./WorldPropState.js";

/**
 * Authoritative room state — single source of truth for client + server.
 * `entities` is keyed by entity id (players use their sessionId).
 * `obstacles` is static (set once); `interactables` mutate (looted).
 */
export class GameRoomState extends Schema {
  @type("number") tick = 0;
  @type("string") status = "playing";
  @type("number") wave = 0;
  @type("number") worldSeed = 0;
  @type({ map: EntityState }) entities = new MapSchema<EntityState>();
  @type([ObstacleState]) obstacles = new ArraySchema<ObstacleState>();
  @type([InteractableState]) interactables = new ArraySchema<InteractableState>();
  @type([WorldPropState]) worldProps = new ArraySchema<WorldPropState>();
}
