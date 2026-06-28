import {
  FRICTION,
  HUNGER_DECAY,
  HUNGER_MAX,
  STAMINA_MAX,
  STARVE_DAMAGE,
  THIRST_DECAY,
  THIRST_MAX,
  TICK_MS,
  WORLD_HALF,
} from "./constants.js";
import { Rng } from "./rng.js";
import { clampBody } from "./movement.js";
import { stepMovement } from "./parkour.js";
import type { BlockWorld } from "./blockWorld.js";
import { groundHeightForLocomotion } from "./world.js";
import { generateWorld } from "./world.js";
import { generatePathGrid, type PathGrid } from "./paths.js";
import { placeWorldProps } from "./worldProps.js";
import { getClass } from "./content/classes.js";
import { getAbility } from "./content/abilities.js";
import { getArchetype } from "./content/npcs.js";
import { itemForGather } from "../content/items/registry.js";
import { canCraft, getCraftRecipe } from "../content/items/recipes.js";
import { decideState, desiredDir } from "./ai/brain.js";
import type { NpcAi } from "./ai/types.js";
import type { AbilityDef } from "./abilities/types.js";
import {
  ABILITY_SLOTS,
  type RegionId,
  DEFAULT_REGION,
  abilityBitsFromInput,
  decodeCraftIntent,
  type EntityKind,
  type EntitySnapshot,
  type InputState,
  type InteractableKind,
  type InteractableSnapshot,
  type InventoryItem,
  type MatchStatus,
  type MovementState,
  type Obstacle,
  type WorldContext,
  type WorldPropSnapshot,
  type WorldSnapshot,
  emptyInput,
} from "./types.js";

/** Internal mutable entity. Plain object for determinism (no Three.js/Colyseus). */
export interface SimEntity {
  id: string;
  kind: EntityKind;
  classId: string;
  x: number;
  y: number; // vertical (parkour)
  z: number;
  vx: number;
  vz: number;
  vy: number;
  rot: number;
  hp: number;
  maxHp: number;
  dead: boolean;
  team: number;
  ownerId: string;
  radius: number;
  speed: number;
  // parkour / survival
  state: MovementState;
  stamina: number;
  maxStamina: number;
  hunger: number;
  thirst: number;
  worldContext: WorldContext;
  missionId: number;
  wallet: number;
  inventory: InventoryItem[];
  // combat
  cooldowns: number[]; // timeMs when each ability slot becomes ready
  damage: number; // projectiles: damage on hit
  lifeMs: number; // projectiles: remaining lifetime
  aoeRadius: number; // projectiles: explosion radius (0 = direct hit only)
  aoeDamage: number;
  knockback: number;
  dashUntilMs: number; // players: dash active until this timeMs
  dashVx: number;
  dashVz: number;
}

export interface SimOptions {
  seed?: number;
  /** Enable wave-based survival director (NPC waves of increasing difficulty). */
  survival?: boolean;
  /** Logical region/chunk id (overworld/cave); keeps world deterministic per region. */
  regionId?: RegionId;
  /** OASIS mode: overworld pacifico + portales a misiones de combate. */
  peacefulOverworld?: boolean;
}

/**
 * Authoritative, deterministic game simulation. Runs identically on the server
 * and offline in the browser. Advance exactly one fixed tick with step().
 */
export class GameSimulation {
  tick = 0;
  timeMs = 0;
  status: MatchStatus = "playing";
  wave = 0;
  /** Total NPCs killed this match (for progression credit). */
  npcKills = 0;

  readonly rng: Rng;
  protected entities = new Map<string, SimEntity>();
  protected inputs = new Map<string, InputState>();
  protected lastProcessed = new Map<string, number>();
  protected ai = new Map<string, NpcAi>();
  private projCounter = 0;
  private npcCounter = 0;

  private survival: boolean;
  private peacefulOverworld: boolean;
  private nextWaveAtMs = 0;
  private readonly missionPortal = { x: 10, z: -10, radius: 2.4, missionId: 1 };
  private readonly returnPortal = { x: 0, z: 0, radius: 3.0 };

  /** Static world geometry (deterministic from seed). */
  readonly blocks: BlockWorld;
  readonly obstacles: Obstacle[];
  readonly worldSeed: number;
  readonly regionId: RegionId;
  readonly pathGrid: PathGrid;
  readonly worldProps: WorldPropSnapshot[];
  protected interactables: InteractableSnapshot[];

  constructor(opts: SimOptions = {}) {
    const seed = opts.seed ?? 1;
    this.regionId = opts.regionId ?? DEFAULT_REGION;
    const regionSeed = seedForRegion(seed, this.regionId);
    this.worldSeed = regionSeed;
    this.rng = new Rng(regionSeed);
    this.survival = opts.survival ?? false;
    this.peacefulOverworld = opts.peacefulOverworld ?? false;
    const world = generateWorld(regionSeed);
    this.blocks = world.blocks;
    this.obstacles = world.obstacles;
    this.interactables = world.interactables;
    this.pathGrid = generatePathGrid(seed, this.obstacles);
    this.worldProps = placeWorldProps(seed, this.pathGrid, this.obstacles);
  }

  // ---- entity lifecycle -------------------------------------------------

  private blankEntity(id: string, kind: EntityKind): SimEntity {
    return {
      id,
      kind,
      classId: "scout",
      x: 0,
      y: 0,
      z: 0,
      vx: 0,
      vz: 0,
      vy: 0,
      rot: 0,
      hp: 1,
      maxHp: 1,
      dead: false,
      team: 0,
      ownerId: "",
      radius: 0.45,
      speed: 0,
      state: "idle",
      stamina: STAMINA_MAX,
      maxStamina: STAMINA_MAX,
      hunger: HUNGER_MAX,
      thirst: THIRST_MAX,
      worldContext: "overworld",
      missionId: 0,
      wallet: 0,
      inventory: [],
      cooldowns: new Array(ABILITY_SLOTS).fill(0),
      damage: 0,
      lifeMs: 0,
      aoeRadius: 0,
      aoeDamage: 0,
      knockback: 0,
      dashUntilMs: 0,
      dashVx: 0,
      dashVz: 0,
    };
  }

  addPlayer(id: string, classId: string, team = 0, spawn?: { x: number; z: number }): SimEntity {
    const def = getClass(classId);
    const pos = spawn ?? this.spawnPoint();
    const e = this.blankEntity(id, "player");
    e.classId = def.id;
    e.x = pos.x;
    e.z = pos.z;
    e.hp = def.maxHp;
    e.maxHp = def.maxHp;
    e.team = team;
    e.radius = def.radius;
    e.speed = def.moveSpeed;
    if (!this.peacefulOverworld) {
      e.worldContext = "mission";
      e.missionId = 1;
    }
    this.entities.set(id, e);
    this.inputs.set(id, emptyInput());
    this.lastProcessed.set(id, 0);
    return e;
  }

  addNpc(id: string, classId: string, team = 1, spawn?: { x: number; z: number }): SimEntity {
    const def = getClass(classId);
    const pos = spawn ?? this.spawnPoint();
    const e = this.blankEntity(id, "npc");
    e.classId = def.id;
    e.x = pos.x;
    e.z = pos.z;
    e.hp = def.maxHp;
    e.maxHp = def.maxHp;
    e.team = team;
    e.radius = def.radius;
    e.speed = def.moveSpeed;
    this.entities.set(id, e);
    return e;
  }

  /** Spawn an AI-driven NPC from an archetype. Used by the survival director. */
  spawnNpc(archetypeId: string, team = 1, spawn?: { x: number; z: number }): SimEntity {
    const arch = getArchetype(archetypeId);
    const pos = spawn ?? this.spawnEdge();
    const id = `npc-${this.npcCounter++}`;
    const e = this.blankEntity(id, "npc");
    e.classId = arch.id;
    e.x = pos.x;
    e.z = pos.z;
    e.hp = arch.maxHp;
    e.maxHp = arch.maxHp;
    e.team = team;
    e.radius = arch.radius;
    e.speed = arch.moveSpeed;
    this.entities.set(id, e);
    this.ai.set(id, {
      state: "wander",
      targetId: null,
      personality: arch.personality,
      homeX: pos.x,
      homeZ: pos.z,
      wanderAngle: this.rng.range(0, Math.PI * 2),
      wanderUntilMs: 0,
      nextAttackMs: 0,
    });
    return e;
  }

  removeEntity(id: string): void {
    this.entities.delete(id);
    this.inputs.delete(id);
    this.lastProcessed.delete(id);
    this.ai.delete(id);
  }

  protected spawnEdge(): { x: number; z: number } {
    const a = this.rng.range(0, Math.PI * 2);
    const r = WORLD_HALF - 1.5;
    return { x: Math.cos(a) * r, z: Math.sin(a) * r };
  }

  getEntity(id: string): SimEntity | undefined {
    return this.entities.get(id);
  }

  hasPlayer(id: string): boolean {
    const e = this.entities.get(id);
    return !!e && e.kind === "player";
  }

  queueInput(id: string, input: InputState): void {
    if (!this.entities.has(id)) return;
    this.inputs.set(id, input);
  }

  protected spawnPoint(): { x: number; z: number } {
    const r = this.rng.range(2, WORLD_HALF - 2);
    const a = this.rng.range(0, Math.PI * 2);
    return { x: Math.cos(a) * r, z: Math.sin(a) * r };
  }

  // ---- tick -------------------------------------------------------------

  step(): void {
    if (this.status !== "playing") return;
    const dt = TICK_MS / 1000;

    this.applyMovement(dt);
    this.updateSurvival(dt);
    this.updateNpcs(dt);
    this.processAbilities();
    this.processInteractions();
    this.processCrafting();
    this.updateProjectiles(dt);
    this.resolveCollisions();
    if (this.survival) this.updateWaves();

    this.tick++;
    this.timeMs += TICK_MS;
  }

  protected applyMovement(dt: number): void {
    for (const e of this.entities.values()) {
      if (e.kind !== "player" || e.dead) continue;
      const input = this.inputs.get(e.id) ?? emptyInput();
      this.lastProcessed.set(e.id, input.seq);

      if (this.timeMs < e.dashUntilMs) {
        e.x += e.dashVx * dt;
        e.z += e.dashVz * dt;
        clampBody(e);
        const floor = groundHeightForLocomotion(e.x, e.z, this.obstacles, e.radius, this.blocks);
        if (e.y < floor) {
          e.y = floor;
          e.vy = 0;
        }
      } else {
        stepMovement(e, input, this.blocks, this.obstacles, dt);
      }
    }
  }

  /** Hunger/thirst decay; starvation damage when either hits zero. */
  protected updateSurvival(dt: number): void {
    for (const e of this.entities.values()) {
      if (e.kind !== "player" || e.dead) continue;
      e.hunger = Math.max(0, e.hunger - HUNGER_DECAY * dt);
      e.thirst = Math.max(0, e.thirst - THIRST_DECAY * dt);
      if (e.hunger <= 0 || e.thirst <= 0) {
        this.damage(e, STARVE_DAMAGE * dt);
      }
    }
  }

  /** Gather/loot: a player holding `action` in range of an unlooted node. */
  protected processInteractions(): void {
    for (const e of this.entities.values()) {
      if (e.kind !== "player" || e.dead) continue;
      const input = this.inputs.get(e.id);
      if (!input || !input.action) continue;
      if (this.peacefulOverworld) {
        if (this.tryToggleMissionContext(e)) continue;
        if (e.worldContext === "mission") continue;
      }
      for (const node of this.interactables) {
        if (node.looted) continue;
        const dist = Math.hypot(node.x - e.x, node.z - e.z);
        if (dist <= node.radius + e.radius) {
          node.looted = true;
          this.grantLoot(e, node.kind);
          break; // one node per tick
        }
      }
    }
  }

  private tryToggleMissionContext(e: SimEntity): boolean {
    if (e.worldContext === "overworld") {
      const d = Math.hypot(e.x - this.missionPortal.x, e.z - this.missionPortal.z);
      if (d <= this.missionPortal.radius + e.radius) {
        e.worldContext = "mission";
        e.missionId = this.missionPortal.missionId;
        e.x = 0;
        e.z = 0;
        e.vx = 0;
        e.vz = 0;
        return true;
      }
      return false;
    }
    const d = Math.hypot(e.x - this.returnPortal.x, e.z - this.returnPortal.z);
    if (d <= this.returnPortal.radius + e.radius) {
      e.worldContext = "overworld";
      e.missionId = 0;
      e.x = this.missionPortal.x - 2.5;
      e.z = this.missionPortal.z + 1.8;
      e.vx = 0;
      e.vz = 0;
      return true;
    }
    return false;
  }

  private grantLoot(e: SimEntity, kind: string): void {
    const itemId =
      itemForGather(kind as InteractableKind)?.id ??
      (kind === "tree" ? "wood" : kind === "rock" ? "stone" : "supply");
    const existing = e.inventory.find((i) => i.itemId === itemId);
    if (existing) existing.qty += 1;
    else e.inventory.push({ itemId, qty: 1 });
    // Containers restore a bit of survival need (placeholder loop).
    if (kind === "container") {
      e.hunger = Math.min(HUNGER_MAX, e.hunger + 25);
      e.thirst = Math.min(THIRST_MAX, e.thirst + 25);
    }
  }

  protected clampToWorld(e: SimEntity): void {
    clampBody(e);
  }

  // ---- combat -----------------------------------------------------------

  protected processAbilities(): void {
    for (const e of this.entities.values()) {
      if (e.kind !== "player" || e.dead) continue;
      if (this.peacefulOverworld && e.worldContext !== "mission") continue;
      const input = this.inputs.get(e.id);
      if (!input || input.abilities === 0) continue;
      const def = getClass(e.classId);
      const abilityBits = abilityBitsFromInput(input.abilities);

      for (let slot = 0; slot < ABILITY_SLOTS; slot++) {
        if ((abilityBits & (1 << slot)) === 0) continue;
        const abilityId = def.abilities[slot];
        if (!abilityId) continue;
        const ability = getAbility(abilityId);
        if (!ability) continue;
        if (this.timeMs < e.cooldowns[slot]) continue;

        this.castAbility(e, ability, input);
        e.cooldowns[slot] = this.timeMs + ability.cooldownMs;
      }
    }
  }

  protected processCrafting(): void {
    for (const e of this.entities.values()) {
      if (e.kind !== "player" || e.dead) continue;
      const input = this.inputs.get(e.id);
      if (!input) continue;
      const recipeId = decodeCraftIntent(input.abilities);
      if (recipeId == null) continue;
      this.tryCraft(e, recipeId);
    }
  }

  private tryCraft(e: SimEntity, recipeId: number): void {
    const recipe = getCraftRecipe(recipeId);
    if (!recipe || !canCraft(e.inventory, recipe)) return;
    for (const req of recipe.ingredients) {
      const slot = e.inventory.find((i) => i.itemId === req.itemId);
      if (!slot) return;
      slot.qty -= req.qty;
    }
    e.inventory = e.inventory.filter((i) => i.qty > 0);
    const out = e.inventory.find((i) => i.itemId === recipe.output.itemId);
    if (out) out.qty += recipe.output.qty;
    else e.inventory.push({ ...recipe.output });
  }

  /** Cast direction: aim if provided, else current facing. */
  private castDir(e: SimEntity, input: InputState): { x: number; z: number } {
    const al = Math.hypot(input.aimX, input.aimZ);
    if (al > 0.1) return { x: input.aimX / al, z: input.aimZ / al };
    return { x: Math.sin(e.rot), z: Math.cos(e.rot) };
  }

  private castAbility(e: SimEntity, ability: AbilityDef, input: InputState): void {
    const dir = this.castDir(e, input);
    e.rot = Math.atan2(dir.x, dir.z);

    switch (ability.kind) {
      case "projectile":
        this.spawnProjectile(e, ability, dir);
        break;
      case "melee":
        this.applyMeleeHit(e, ability, dir);
        break;
      case "aoe":
        this.applyAoe(e.team, e.x, e.z, ability.radius, ability.damage, ability.knockback ?? 0, e.id);
        break;
      case "dash":
        e.dashVx = dir.x * (ability.speed ?? 18);
        e.dashVz = dir.z * (ability.speed ?? 18);
        e.dashUntilMs = this.timeMs + (ability.range / (ability.speed ?? 18)) * 1000;
        if (ability.damage > 0) this.applyMeleeHit(e, ability, dir);
        break;
    }
  }

  private spawnProjectile(
    e: SimEntity,
    ability: AbilityDef,
    dir: { x: number; z: number }
  ): void {
    const id = `proj-${this.projCounter++}`;
    const p = this.blankEntity(id, "projectile");
    const speed = ability.speed ?? 16;
    p.classId = ability.id;
    p.x = e.x + dir.x * (e.radius + 0.3);
    p.z = e.z + dir.z * (e.radius + 0.3);
    p.vx = dir.x * speed;
    p.vz = dir.z * speed;
    p.rot = Math.atan2(dir.x, dir.z);
    p.team = e.team;
    p.ownerId = e.id;
    p.radius = ability.radius;
    p.damage = ability.damage;
    p.knockback = ability.knockback ?? 0;
    p.aoeRadius = ability.aoeRadius ?? 0;
    p.aoeDamage = ability.aoeDamage ?? 0;
    p.lifeMs = (ability.range / speed) * 1000;
    this.entities.set(id, p);
  }

  private applyMeleeHit(
    attacker: SimEntity,
    ability: AbilityDef,
    dir: { x: number; z: number }
  ): void {
    const arc = ability.arc ?? Math.PI / 2;
    for (const t of this.entities.values()) {
      if (t.dead || t.id === attacker.id) continue;
      if (t.kind === "projectile" || t.team === attacker.team) continue;
      const dx = t.x - attacker.x;
      const dz = t.z - attacker.z;
      const dist = Math.hypot(dx, dz);
      if (dist > ability.range + t.radius) continue;
      const toAngle = Math.atan2(dx, dz);
      const aimAngle = Math.atan2(dir.x, dir.z);
      if (Math.abs(angleDiff(aimAngle, toAngle)) > arc) continue;
      this.damage(t, ability.damage);
      this.knock(t, dx, dz, dist, ability.knockback ?? 0);
    }
  }

  private applyAoe(
    srcTeam: number,
    x: number,
    z: number,
    radius: number,
    damage: number,
    knockback: number,
    excludeId: string
  ): void {
    for (const t of this.entities.values()) {
      if (t.dead || t.id === excludeId) continue;
      if (t.kind === "projectile" || t.team === srcTeam) continue;
      const dx = t.x - x;
      const dz = t.z - z;
      const dist = Math.hypot(dx, dz);
      if (dist > radius + t.radius) continue;
      this.damage(t, damage);
      this.knock(t, dx, dz, dist, knockback);
    }
  }

  protected updateProjectiles(dt: number): void {
    const dead: string[] = [];
    for (const p of this.entities.values()) {
      if (p.kind !== "projectile") continue;
      p.x += p.vx * dt;
      p.z += p.vz * dt;
      p.lifeMs -= dt * 1000;

      let hit = false;
      for (const t of this.entities.values()) {
        if (t.kind === "projectile" || t.dead) continue;
        if (t.id === p.ownerId || t.team === p.team) continue;
        const dist = Math.hypot(t.x - p.x, t.z - p.z);
        if (dist <= p.radius + t.radius) {
          hit = true;
          break;
        }
      }

      const expired =
        p.lifeMs <= 0 ||
        Math.abs(p.x) > WORLD_HALF ||
        Math.abs(p.z) > WORLD_HALF;

      if (hit || expired) {
        if (p.aoeRadius > 0) {
          this.applyAoe(p.team, p.x, p.z, p.aoeRadius, p.aoeDamage, p.knockback, p.ownerId);
        } else if (hit) {
          for (const t of this.entities.values()) {
            if (t.kind === "projectile" || t.dead) continue;
            if (t.id === p.ownerId || t.team === p.team) continue;
            const dx = t.x - p.x;
            const dz = t.z - p.z;
            const dist = Math.hypot(dx, dz);
            if (dist <= p.radius + t.radius) {
              this.damage(t, p.damage);
              this.knock(t, dx, dz, dist, p.knockback);
              break;
            }
          }
        }
        dead.push(p.id);
      }
    }
    for (const id of dead) this.entities.delete(id);
  }

  protected damage(t: SimEntity, amount: number): void {
    if (this.peacefulOverworld && t.kind === "player" && t.worldContext !== "mission") return;
    if (t.dead || amount <= 0) return;
    t.hp -= amount;
    if (t.hp <= 0) {
      t.hp = 0;
      t.dead = true;
      t.vx = 0;
      t.vz = 0;
    }
  }

  private knock(
    t: SimEntity,
    dx: number,
    dz: number,
    dist: number,
    impulse: number
  ): void {
    if (impulse <= 0) return;
    const d = dist > 0.0001 ? dist : 1;
    t.vx += (dx / d) * impulse;
    t.vz += (dz / d) * impulse;
  }

  /** Simple circle-circle push-apart between solid entities. Deterministic order. */
  protected resolveCollisions(): void {
    const solids = [...this.entities.values()].filter(
      (e) => !e.dead && e.kind !== "projectile"
    );
    solids.sort((a, b) => (a.id < b.id ? -1 : 1));

    for (let i = 0; i < solids.length; i++) {
      for (let j = i + 1; j < solids.length; j++) {
        const a = solids[i];
        const b = solids[j];
        const dx = b.x - a.x;
        const dz = b.z - a.z;
        const distSq = dx * dx + dz * dz;
        const minDist = a.radius + b.radius;
        if (distSq >= minDist * minDist || distSq === 0) continue;
        const dist = Math.sqrt(distSq);
        const overlap = (minDist - dist) / 2;
        const nx = dx / dist;
        const nz = dz / dist;
        a.x -= nx * overlap;
        a.z -= nz * overlap;
        b.x += nx * overlap;
        b.z += nz * overlap;
        clampBody(a);
        clampBody(b);
      }
    }
  }

  // ---- NPC AI -----------------------------------------------------------

  protected updateNpcs(dt: number): void {
    // Remove dead NPCs so the wave director can detect a cleared wave.
    for (const id of [...this.ai.keys()]) {
      const e = this.entities.get(id);
      if (e?.dead) {
        this.npcKills++;
        this.awardMissionKill(e.missionId || 1, 12);
      }
      if (!e || e.dead) this.removeEntity(id);
    }

    const k = Math.min(1, FRICTION * dt);

    for (const [id, ai] of this.ai) {
      const e = this.entities.get(id);
      if (!e || e.dead) continue;

      const target = this.findNearestEnemy(e, ai.personality.visionRange);
      const hpFrac = e.hp / e.maxHp;
      const tInfo = target
        ? {
            x: target.x,
            z: target.z,
            radius: target.radius,
            dist: Math.hypot(target.x - e.x, target.z - e.z),
          }
        : null;

      ai.state = decideState(ai, hpFrac, tInfo);
      ai.targetId = target?.id ?? null;

      if (ai.state === "wander" && this.timeMs >= ai.wanderUntilMs) {
        ai.wanderAngle = this.rng.range(0, Math.PI * 2);
        ai.wanderUntilMs = this.timeMs + this.rng.range(800, 2200);
      }

      // Attack: deal personality damage on cooldown when in range.
      if (ai.state === "attack" && target && this.timeMs >= ai.nextAttackMs) {
        this.damage(target, ai.personality.attackDamage);
        const dx = target.x - e.x;
        const dz = target.z - e.z;
        this.knock(target, dx, dz, Math.hypot(dx, dz), 2);
        ai.nextAttackMs = this.timeMs + ai.personality.attackCdMs;
      }

      const dir = desiredDir(ai.state, e, ai, tInfo);
      let desVx = 0;
      let desVz = 0;
      if (dir) {
        desVx = dir.x * e.speed;
        desVz = dir.z * e.speed;
      }

      // Group tactics: light separation so they surround instead of stacking.
      const sep = this.separation(e);
      desVx += sep.x * e.speed * 0.6;
      desVz += sep.z * e.speed * 0.6;

      e.vx += (desVx - e.vx) * k;
      e.vz += (desVz - e.vz) * k;
      e.x += e.vx * dt;
      e.z += e.vz * dt;
      clampBody(e);

      const moving = Math.hypot(e.vx, e.vz);
      if (tInfo && (ai.state === "attack" || ai.state === "chase")) {
        e.rot = Math.atan2(tInfo.x - e.x, tInfo.z - e.z);
      } else if (moving > 0.05) {
        e.rot = Math.atan2(e.vx, e.vz);
      }
    }
  }

  private findNearestEnemy(e: SimEntity, range: number): SimEntity | null {
    let best: SimEntity | null = null;
    let bestDist = range;
    for (const o of this.entities.values()) {
      if (o.dead || o.kind !== "player" || o.team === e.team) continue;
      const d = Math.hypot(o.x - e.x, o.z - e.z);
      if (d < bestDist) {
        bestDist = d;
        best = o;
      }
    }
    return best;
  }

  /** Steer away from nearby same-kind NPCs to avoid clumping. */
  private separation(e: SimEntity): { x: number; z: number } {
    let sx = 0;
    let sz = 0;
    const desired = e.radius * 2.4;
    for (const o of this.entities.values()) {
      if (o === e || o.kind !== "npc" || o.dead) continue;
      const dx = e.x - o.x;
      const dz = e.z - o.z;
      const d = Math.hypot(dx, dz);
      if (d > 0.0001 && d < desired) {
        const w = (desired - d) / desired;
        sx += (dx / d) * w;
        sz += (dz / d) * w;
      }
    }
    return { x: sx, z: sz };
  }

  // ---- survival waves ---------------------------------------------------

  private updateWaves(): void {
    if (this.peacefulOverworld) {
      const missionPlayers = [...this.entities.values()].filter(
        (e) => e.kind === "player" && !e.dead && e.worldContext === "mission"
      );
      if (missionPlayers.length === 0) {
        for (const id of [...this.ai.keys()]) this.removeEntity(id);
        this.nextWaveAtMs = 0;
        return;
      }
    }
    const playersTotal = this.playerCount();
    if (playersTotal > 0 && this.aliveCount("player") === 0) {
      this.status = "ended";
      return;
    }
    if (this.aliveCount("npc") > 0) return;

    if (this.nextWaveAtMs === 0) {
      this.nextWaveAtMs = this.timeMs + (this.wave === 0 ? 1500 : 4000);
      return;
    }
    if (this.timeMs >= this.nextWaveAtMs) {
      this.wave++;
      this.spawnWave(this.wave);
      this.nextWaveAtMs = 0;
    }
  }

  /** Spawn a wave: more and tougher enemies as waves climb. */
  spawnWave(wave: number): void {
    const count = Math.min(3 + wave * 2, 24);
    for (let i = 0; i < count; i++) {
      const npc = this.spawnNpc(this.pickArchetype(wave));
      npc.worldContext = "mission";
      npc.missionId = 1;
    }
  }

  private awardMissionKill(missionId: number, amount: number): void {
    for (const e of this.entities.values()) {
      if (e.kind !== "player" || e.dead) continue;
      if (e.worldContext === "mission" && e.missionId === missionId) {
        e.wallet += amount;
      }
    }
  }

  private pickArchetype(wave: number): string {
    const r = this.rng.next();
    if (wave >= 5 && r < 0.18) return "brute";
    if (wave >= 3 && r < 0.45) return "skirmisher";
    return "grunt";
  }

  // ---- snapshot ---------------------------------------------------------

  snapshot(): WorldSnapshot {
    const entities: EntitySnapshot[] = [];
    for (const e of this.entities.values()) {
      entities.push({
        id: e.id,
        kind: e.kind,
        classId: e.classId,
        x: e.x,
        y: e.y,
        z: e.z,
        vx: e.vx,
        vz: e.vz,
        rot: e.rot,
        hp: e.hp,
        maxHp: e.maxHp,
        dead: e.dead,
        team: e.team,
        ownerId: e.ownerId,
        state: e.state,
        stamina: e.stamina,
        maxStamina: e.maxStamina,
        hunger: e.hunger,
        thirst: e.thirst,
        worldContext: e.worldContext,
        missionId: e.missionId,
        wallet: e.wallet,
      });
    }
    const lastProcessedInput: Record<string, number> = {};
    for (const [id, seq] of this.lastProcessed) lastProcessedInput[id] = seq;

    return {
      tick: this.tick,
      timeMs: this.timeMs,
      status: this.status,
      wave: this.wave,
      entities,
      obstacles: this.obstacles,
      interactables: this.interactables,
      worldSeed: this.worldSeed,
      worldProps: this.worldProps,
      lastProcessedInput,
    };
  }

  getInteractables(): InteractableSnapshot[] {
    return this.interactables;
  }

  getWorldProps(): WorldPropSnapshot[] {
    return this.worldProps;
  }

  getPathGrid(): PathGrid {
    return this.pathGrid;
  }

  playerCount(): number {
    let n = 0;
    for (const e of this.entities.values()) if (e.kind === "player") n++;
    return n;
  }

  aliveCount(kind: EntityKind): number {
    let n = 0;
    for (const e of this.entities.values()) {
      if (e.kind === kind && !e.dead) n++;
    }
    return n;
  }
}

function seedForRegion(seed: number, regionId: RegionId): number {
  if (regionId.kind === "cave") {
    return seed * 977 + 10_000 + (regionId.caveId & 0xfff);
  }
  const gx = regionId.gx + 2;
  const gz = regionId.gz + 2;
  return seed * 977 + gx * 101 + gz * 313;
}

function angleDiff(a: number, b: number): number {
  let d = b - a;
  while (d > Math.PI) d -= Math.PI * 2;
  while (d < -Math.PI) d += Math.PI * 2;
  return d;
}
