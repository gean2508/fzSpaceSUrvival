---
name: swarm-sim
description: Run the native game's automated flight/QA drivers on the desktop (screenshots + invariant checks) to reproduce and verify bugs without the phone, and the roadmap for mass N-agent simulation that doubles as NPC-AI behaviour data. Use when asked to test flight/landing/rocket, reproduce a soft-lock, verify a fix visually, or run stress/swarm simulations.
---

# swarm-sim — automated QA drivers for the native game

The game runs on THIS machine and drives itself through the real input pipeline,
taking screenshots and checking invariants. Use it to reproduce bugs and verify
fixes without the Redmi.

## Run a driver (desktop, headed)

```bash
cd native && cargo build -q -p ha-game
# Creative ascent→orbit→descend→touchdown round trip + landing invariants:
timeout 75 env HA_FLYTEST=1  HA_AUTOPLAY=1 WGPU_BACKEND=gl ./target/debug/ha-game
# Moon climb to CLEAR orbit (no white-square loop) + disc-scale screenshots:
timeout 75 env HA_MOONTEST=1 HA_AUTOPLAY=1 WGPU_BACKEND=gl ./target/debug/ha-game
# Rocket ignition→space→descent→touchdown, screenshots both legs:
timeout 75 env HA_ROCKETTEST=1 HA_AUTOPLAY=1 WGPU_BACKEND=gl ./target/debug/ha-game
# Land→take-off→land twice on Moon/Venus/Mars (no orbital soft-lock):
timeout 200 env HA_CYCLETEST=1 HA_AUTOPLAY=1 WGPU_BACKEND=gl ./target/debug/ha-game
# THE round trip: fly Earth→Moon, land, take off, fly back, land (frame rebase
# + zero camera jumps). This is the "I can't get back to Earth" report as a test:
timeout 240 env HA_TRIPTEST=1 HA_AUTOPLAY=1 WGPU_BACKEND=gl ./target/debug/ha-game
```

- Screenshots + logs land in `FLYTEST_DIR` (const in `crates/ha-game/src/debug/
  autodrive.rs`) — **point it at the current session scratchpad** before running,
  then `Read` the PNGs.
- Exit code 0 = pass, ≠0 = a checked invariant failed (the log says which).
- Grep the log for `INVARIANT`, `CONTINUITY`, `STUCK`, `DONE`, ignore `B0003`
  despawn warnings (benign).

## Principles (why this exists)

- **Close full cycles.** A test that only ascends validates ~40% of the flight
  system. Drivers must land→take-off→land like a real player. The landing
  invariants (region back to surface, `RocketFlight` inactive, `view_h`≈surface,
  player visible) catch the "camera leaves, player stuck in orbit" soft-lock.
- **Deterministic repro > guessing.** When a user reports a flow bug, add the
  flow + invariant to a driver so it fails in CI-style, fix, confirm green.

## Roadmap — `HA_SWARM=N` (mass agents = NPC-AI data)

Not built yet; design in `PLAN_VUELO_ESPACIO_Y_TESTS.md` §3. Headless mode that
instantiates N agents through the SAME `step_topdown_movement`/collision pipeline
with simple policies (wander / seek-resource / flee), no render, max speed.
Doubles as:
- **Stress QA**: thousands of agents × minutes surface panics/soft-locks a lone
  player would take hours to hit.
- **NPC-AI dataset**: logging trajectories/decisions/outcomes (died? reached
  resource? stuck on a tree?) seeds the colonists' behaviour — calibrate utility
  from real world stats, flag unfair map cells, baseline for imitation later.

Phases: F1 headless N-agents + panic/softlock report · F2 trajectory log to disk
· F3 utility-based NPC policy calibrated on it.
