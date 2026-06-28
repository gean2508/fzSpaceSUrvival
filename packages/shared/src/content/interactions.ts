/** Target kinds for hold-interact / look targeting (extends sim interactables). */
export type InteractTargetKind =
  | "rock"
  | "tree"
  | "water"
  | "entity"
  | "container"
  | "launch_module"
  | "launch_ignition"
  | "space_reentry"
  | "space_transit_luna"
  | "space_station_module"
  | "lunar_return";

export type InteractActionKind =
  | "mine"
  | "attack"
  | "collect"
  | "gather"
  | "none"
  | "assemble"
  | "launch"
  | "return_earth"
  | "travel_luna";

export interface InteractionRule {
  equippedItemId: string;
  targetKind: InteractTargetKind;
  kind: InteractActionKind;
  durationMs: number;
  itemId?: string;
  damage?: number;
}

/** Max reach along look ray (world units). Mirrored in ha-shared. */
export const INTERACT_REACH = 4.5;

/** Gesture thresholds (ms / px) — client reads same values via ha-shared mirror. */
export const GESTURE_HOLD_START_MS = 200;
export const GESTURE_TAP_MAX_MS = 150;
export const GESTURE_MOVE_CANCEL_PX = 14;

export const INTERACTION_RULES: InteractionRule[] = [
  { equippedItemId: "pickaxe", targetKind: "rock", kind: "mine", durationMs: 800, itemId: "stone" },
  { equippedItemId: "axe", targetKind: "tree", kind: "gather", durationMs: 1200, itemId: "wood" },
  { equippedItemId: "sword_iron", targetKind: "entity", kind: "attack", durationMs: 400, damage: 18 },
  { equippedItemId: "sword_steel", targetKind: "entity", kind: "attack", durationMs: 350, damage: 26 },
  { equippedItemId: "watering_can", targetKind: "water", kind: "collect", durationMs: 600, itemId: "supply" },
  { equippedItemId: "hand", targetKind: "tree", kind: "gather", durationMs: 2200, itemId: "wood" },
  { equippedItemId: "hand", targetKind: "rock", kind: "none", durationMs: 0 },
  { equippedItemId: "hand", targetKind: "container", kind: "gather", durationMs: 900, itemId: "supply" },
  { equippedItemId: "hand", targetKind: "launch_module", kind: "assemble", durationMs: 3500 },
  { equippedItemId: "hand", targetKind: "launch_ignition", kind: "launch", durationMs: 2500 },
  { equippedItemId: "hand", targetKind: "space_reentry", kind: "return_earth", durationMs: 2200 },
  { equippedItemId: "hand", targetKind: "space_transit_luna", kind: "travel_luna", durationMs: 1800 },
  { equippedItemId: "hand", targetKind: "space_station_module", kind: "assemble", durationMs: 3000 },
  { equippedItemId: "hand", targetKind: "lunar_return", kind: "return_earth", durationMs: 2000 },
];

const ruleKey = (equipped: string, target: InteractTargetKind) => `${equipped}:${target}`;

const RULE_MAP = new Map<string, InteractionRule>(
  INTERACTION_RULES.map((r) => [ruleKey(r.equippedItemId, r.targetKind), r])
);

/** Resolve equipped×target. Empty hands use item id `"hand"`. */
export function resolveInteraction(
  equippedItemId: string | null | undefined,
  targetKind: InteractTargetKind
): InteractionRule | null {
  const equipped = equippedItemId?.trim() ? equippedItemId : "hand";
  const exact = RULE_MAP.get(ruleKey(equipped, targetKind));
  if (exact) return exact;
  if (equipped !== "hand") {
    return RULE_MAP.get(ruleKey("hand", targetKind)) ?? null;
  }
  return null;
}

/** Deterministic duration for tests (no RNG). */
export function interactionDurationMs(
  equippedItemId: string,
  targetKind: InteractTargetKind
): number {
  return resolveInteraction(equippedItemId, targetKind)?.durationMs ?? 0;
}
