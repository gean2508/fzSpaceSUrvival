import { Schema, type } from "@colyseus/schema";

/** Static decorative prop synced once at room create. */
export class WorldPropState extends Schema {
  @type("string") id = "";
  @type("string") kind = "house";
  @type("number") x = 0;
  @type("number") z = 0;
  @type("number") rot = 0;
}
