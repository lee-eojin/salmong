# salmong: design plan

Status: reviewed 2026-11-05 (section 10). Design only, no code exists yet. Numbers marked *(tune)* are starting
values. All budgets are unmeasured targets until M0 or M5.

salmong is a bus-route puzzle in the same genre as Mini Metro. It shares mechanics only: names, art, audio,
UI look and maps are our own.

## 1. Decisions at a glance

| Topic | Decision | Why |
|---|---|---|
| Sim location | Pure Rust `salmong-core`, no Bevy. Bevy holds a `Sim` resource and mirrors it | Headless tests, bots, replays, 1:1 mapping to the TLA+ spec |
| Numbers in core | Integers only | Native and WASM replays match bit for bit; Bend jobs need only `U32` |
| Passenger state | Stateless: `want` shape only, no stored path | Network edits cannot leave stale plans; rerouting is a table swap |
| Runtime router | Rust, in core, behind `Router` trait | Bend has no supported WASM target (section 5) |
| Bend role | Offline scenario generation only: writes the versioned pack the game loads | Section 5, Q1. This narrows the README's "routing in Bend" |
| TLA+ role | `Boarding.tla` gates the boarding rules before they are coded | Rules are cheap to change in a spec, costly after the UI exists |
| Passengers in ECS | Not entities. Instance data only | Keeps entity count under 200 |
| Web backend | WebGPU only in MVP, no WebGL2 fallback (Q2) | Vertex instancing keeps a later fallback possible |

## 2. MVP rules

- **World.** Bounded integer grid, starting with 3 stations of 3 different shapes: circle, triangle, square; star only after
  week 2. One more station every 20 s *(tune)*, seeded, minimum spacing, up to 64. The scenario pack fixes the schedule.
- **Passenger spawn.** Per-station rate that ramps with time from the pack (section 5). A passenger wants a shape that
  differs from its station's and exists on the map. Spawning pauses while 5,000 passengers are alive (section 6).
- **Routes.** The player drags between stations: an ordered list, no repeats, at least 2 stops. Extend at either
  end, insert a stop, or remove one. Lines only, no loops (Q4). Start with 3 routes (max 8), 4 buses (max 32).
- **Buses.** Capacity 6 *(tune)*, at least 1 per route, so `AddRoute` needs a free bus. A bus shuttles end to end. Its
  cycle is `Moving -> Alighting -> Boarding -> Moving`. After alighting, a bus whose `dir` has no next stop reverses, so
  boarders see the new direction.
- **Overflow and game over.** Station queues have no limit, so alighting never blocks and the full-bus, full-station
  deadlock cannot occur. Station capacity 8 *(tune)*. While `queue > capacity` the station timer rises 1 per tick;
  otherwise it falls 2 per tick, down to 0. At 1350 ticks (45 s) the game is over.
- **Score.** Passengers delivered. Tie-break by survival time.
- **Weekly rewards (optional, M6).** Week = 140 s *(tune)*. The sim pauses and the player picks 1 of 2: +1 route,
  +2 buses, +2 bus capacity on one route, +4 capacity at one station.

**Routing table.** Recomputed only when the network changes, swapped atomically between ticks. Hop `H = 10`, transfer
`T = 30` *(tune)*. `INF = u32::MAX` and additions saturate. Stops never repeat, so `(r, d, i)` names one stop of route
`r` in direction `d`, with `st` its station and `n` the following stop (none at the end of `d`). Edges per wanted shape:
board `Wait(st) -> Ride(r,d,i)` costs `T`, only where `n` exists; hop `Ride(r,d,i) -> Ride(r,d,n)` costs `H`; alight
`Ride(r,d,i) -> Wait(st)` costs 0 and is the only edge at a terminal, so reversal is alight plus reboard and pays `T`.

```text
cost[st]    = 0 if shape(st) = want, else min of T + ride[r,d,i] over boardable stops at st, else INF
ride[r,d,i] = 0 if shape(st) = want; cost[st] if no n; else min(cost[st], H + ride[r,d,n])
board[st]   = mask of (r,d) with cost[st] < INF and T + ride[r,d,i] = cost[st]    (u16: 8 routes x 2 directions)
stay[r,d,i] = n exists and H + ride[r,d,n] <= cost[st]                            (ties stay on)
```

All optimal hops are stored, so the table has one correct value and routers compare by exact equality. A rider with no
path stays on to the terminal (`INF <= INF`), then waits. **Progress measure:** potential is `cost[st]` while waiting and
`ride[r,d,i]` while riding. Under one table, board lowers it by `T`, hop by `H`, and alight keeps it equal. Alight is
always followed by a board, so each ride ends strictly lower, terminals included.

**Alighting (always first).** Each rider on a stopped bus, in boarding order: if the station shape equals `want`, delivered,
+1 score; else if `stay` is false, alight to the back of the queue (a transfer); else stay on board.
**Boarding (after alighting).** While seats remain, take the oldest queued passenger whose `board[station]` contains
this bus's `(route, dir)`. Ineligible passengers are skipped, so there is no head-of-line blocking. A passenger with
`cost = INF` just waits; that queue pressure is the intended penalty. Stations keep one FIFO per wanted shape, stamped
with a queue-entry `seq`, so the oldest eligible passenger is the lowest `seq` among at most 4 heads.

**Service and ordering.** Each bus has its own timer. It starts at 8 ticks *(tune)* on entering `Alighting`, counts down
while stopped, and at 0 moves one passenger and resets. A phase with nothing left to do ends in the same tick; a bus with no
free seat or nobody eligible departs. Each tick runs all buses through alight, then all through board, by ascending `BusId`:
a transfer can board another bus that tick, and the lower `BusId` wins a contested passenger. Boarding fills seats and nobody
alights during it, so free seats bound dwell.

**Network edits.** A command that fails validation changes nothing (section 3). Stations are permanent and a moving bus
owns its segment `(from, to)`, so movement never depends on the route. Unloaded riders follow the normal rules.
- **Any edit:** a moving bus finishes its segment. A stopped bus on an edited route restarts at `Alighting` under the new
  table. `dir` is kept and the next stop is read from the new list.
- **Bus at a stop no longer on its route:** all riders alight (delivered if the shape matches), nobody boards, and the
  bus heads for the nearest stop on the list (squared grid distance, ties to lowest index), `Fwd` unless that is the last.
- **`RemoveRoute`:** the route and its slot leave the network at once. Its buses become `Retiring`, hold no route and
  ignore later edits: they finish any segment, unload everyone at the timer rate with no boarding, then go to inventory.

## 3. Workspace and modules

```text
Cargo.toml               workspace, resolver 3, edition 2024; rust-toolchain.toml pins 1.97.1 and wasm32-unknown-unknown
crates/salmong-core/     no Bevy, no I/O, near-zero deps
crates/salmong-game/     Bevy bin + lib, native and wasm
crates/salmong-pack/     serde code for the scenario pack: parse_pack, write_pack. Native and wasm
crates/salmong-oracle/   native only: Rust reference generator, ReferenceRouter. Never in the wasm graph
bend/, assets/scenarios/ scenarios.bend; pack.v1.json, pack.v1.sha256
spec/                    Boarding.tla, Boarding.{safety,live,fault}.cfg, BoardingTrace.tla
web/index.html           Trunk entry, navigator.gpu check, error screen
```

`salmong-core` modules: `ids`, `rng` (PCG), `balance`, `scenario`, `network`, `routing`, `boarding`, `spawn`, `overflow`, `rewards`,
`sim`, `event`, `replay`, `trace`. Use `Vec`, `VecDeque` and `BTreeMap` only: `HashMap` order would break determinism.

Key types and signatures (sketch):
```rust
pub struct StationId(u16); pub struct RouteId(u8); pub struct BusId(u16); pub struct PassengerId(u32); pub struct Tick(u64);
pub enum Shape { Circle, Triangle, Square, Star }  pub enum Dir { Fwd, Back }  pub enum SimMode { Running, Reward, Over }
pub struct Passenger { pub id: PassengerId, pub want: Shape, pub born: Tick }
pub enum BusMode { Active { route: RouteId, dir: Dir }, Retiring }
pub enum BusPhase { Moving { from: StationId, to: StationId, progress: u32 }, Alighting, Boarding }
pub enum Command { AddRoute { stops: Vec<StationId> }, EditRoute { route: RouteId, stops: Vec<StationId> },
                   RemoveRoute { route: RouteId }, AssignBus { route: RouteId }, ChooseReward { index: u8 } }
pub enum SimEvent { StationSpawned, Boarded, Alighted, Delivered, OverflowStarted, RewardOffered, CommandRejected, GameOver }
pub trait Router { fn solve(&self, net: &NetworkSnapshot) -> RouteTable; }
pub struct DijkstraRouter;                                 // runtime, reverse Dijkstra per shape
impl Sim {
    pub fn new(scenario: Scenario, seed: u64, balance: Balance) -> Self;
    pub fn queue(&mut self, cmd: Command);                 // runs at the start of the next tick
    pub fn step(&mut self, events: &mut Vec<SimEvent>);    // one fixed tick
    pub fn view(&self) -> SimView<'_>;                     // read-only, for rendering; includes SimMode
    pub fn state_bytes(&self) -> Vec<u8>;                  // canonical encoding
    pub fn state_hash(&self) -> u64;                       // hash of state_bytes
}
```

- **Router contract.** `solve` takes a snapshot within limits and returns a table with `table.rev == net.rev` satisfying
  section 2. `Sim` calls `DijkstraRouter` directly; test-only `ReferenceRouter` (Bellman-Ford) is compared table to table, so no injection.
- **Tick order.** Run queued commands, recompute the table if dirty, spawn, move buses, alight, board, overflow, rewards.
  Commands run in queue order and are validated when they run, so an earlier one can invalidate a later one; a rejected
  command changes nothing and emits `CommandRejected`. In `SimMode::Reward` a tick runs commands only.
- **Replay.** Header: `RULES_VERSION` (bumped by any rule or routing change), pack sha256, scenario `id`, `seed`, `Balance`.
  Body: `[(Tick, Command)]` in execution order. `state_bytes` is fixed-order little-endian and covers pending commands,
  RNG state, timers and `seq` counters.

`salmong-game` ECS. Core owns all rules; ECS holds presentation only (`StationView`, `BusView`, `RouteView`, `SimRes`).
Bevy: pin `=0.20.1`, `default-features = false`. The default set pulls `3d` and `audio`, which we do not need yet.
- **States:** `Loading`, `Menu`, `Playing`, `Reward`, `GameOver`. The last three only follow `view().mode`.
- **FixedUpdate (30 Hz):** `apply_commands -> step_sim -> dispatch_events`. `apply_commands` drains `PendingCommands`
  into `Sim::queue`, so every input belongs to one explicit tick.
- **Update:** input, drag preview, `sync_views`, `interpolate_buses`, on-change mesh and HUD rebuilds, instance buffers.
  Buses blend previous and current tick snapshots by the fixed overstep fraction. A bus with no previous snapshot snaps.
  Edits cause no jumps, because segments always finish.

## 4. TLA+ usage

`spec/Boarding.tla` models section 2 at the level of whole actions, not geometry.
- **Constants:** `Stations`, `Routes`, `Buses`, `Capacity`, `MaxPassengers`, `MaxEdits`, `Fault`, and `Networks`: a
  hand-listed set of route maps, including adversarial edits and disconnected networks.
- **Variables:** `network`, `dir`, `retiring`, `queue`, `onboard`, `busAt`, `busPhase`, `delivered`, `edits`. The table is
  an operator `Table(network)` implementing the section 2 equations, so only realizable tables exist.
- **Actions:** `Spawn`, `Move(b)`, `Alight(b)`, `Board(b)`, `Depart(b)`, `EditNetwork` (to any `Networks` member, retiring buses).
- **Safety:** invariants `CapacitySafe` (a bus holds at most `Capacity`) and `Conserved` (spawned = waiting + onboard + delivered,
  each passenger in one place); action properties `NoWrongDelivery` (only at the wanted shape), `AlightBeforeBoard` (no
  `Board(b)` while `b` holds a rider who should alight), `TransferSound` (bus to queue at that bus's stop, in one step).

Liveness, under weak fairness per bus on each action, once edits stop (`edits` is bounded, `EditNetwork` is not fair).
`Board(b)` takes the oldest eligible passenger and `Depart(b)` is disabled while one fits, so per-passenger fairness is
not needed. The spec has no overflow: game over ends these guarantees.
- `Delivery`: after the last edit, a passenger with finite potential under the final table is eventually delivered.
- `BusProgress`: every bus leaves every stop. `NoPingPong`: potential (section 2) falls on every `Board` and hop and stays
  equal on `Alight`, so no cycles.

Start with 4 stations, 2 routes, 2 buses, capacity 2, 3 passengers, 1 edit. Arrivals are bounded by `MaxPassengers`;
liveness under unbounded arrivals would need a separate FIFO argument. Use symmetry sets only in the safety config,
because TLC can miss liveness violations under symmetry. `Boarding.fault.cfg` sets `Fault` to one deliberate bug at a time
(board at `Capacity`, board before alight, split transfer); each must give a counterexample on its property. Passing runs
report completed exploration and state count. Run: `java -cp "$TLA2TOOLS" tlc2.TLC -workers auto -config <cfg> Boarding.tla`.

TLC proves the spec, not the Rust. Two checks close the gap, both on a verification trace separate from `SimEvent`.
With the `trace` feature, core logs every microstep as `(tick, action, bus, passenger, before, after)` with stable
`PassengerId`s, and each microstep maps to one TLA+ action (each rider moved is one `Alight` or `Board`). Proptests assert
the same invariants after every microstep, and `BoardingTrace.tla` checks traces against `Next`. Corrupted traces must fail.

## 5. Bend usage

**Fact check (read 2026-11-04, Bend source `4024f04`, nothing executed here).** Bend 2 runs in a browser as JavaScript on
[one core](https://github.com/bendlang/bend/blob/4024f04/README.md#L239). No upstream Bend or HVM2 release targets WASM.
- Bend 2 is [`bendlang/bend`](https://github.com/bendlang/bend) ([v2.0.39](https://github.com/bendlang/bend/releases/tag/v2.0.39)); crates.io has the old [`Bend1`](https://github.com/HigherOrderCO/Bend1) line, which [does not carry over](https://github.com/bendlang/bend/blob/4024f04/README.md#L223).
- Targets are [C, Metal, CUDA and JavaScript](https://github.com/bendlang/bend/blob/4024f04/README.md#L238). [PR #866](https://github.com/bendlang/bend/pull/866) (WASM) closed unmerged 2026-09-24; [#1352](https://github.com/bendlang/bend/issues/1352), [#1598](https://github.com/bendlang/bend/issues/1598) are open. [HVM2](https://github.com/HigherOrderCO/HVM2/blob/6542760/README.md): C and CUDA only.

**Decision.** Bend stays out of the shipped runtime: routing is tiny (64 stations, 4 shapes, edits only), and a young compiler
("not yet fully audited") should not sit on every player's critical path. Bend's one job is **offline scenario generation**: a
pure function of a seed, needing no `Sim`, with a small Rust reference. At 16 scenarios parallelism earns nothing; the job proves
the pipe. Not planned: route-layout sweeps, operator AI, a Bend routing oracle (Q1). Fallback: M4.6 (the Rust reference writes the same bytes).

**File format: `assets/scenarios/pack.v1.json`, committed.** Bend has no JSON library, but integer JSON is just string
joins, and git and `cmp` can read it. Canonical bytes: ASCII, integers only, no spaces, fixed key order as shown, scenarios
one per line joined by `,\n`, final `]}\n`. `ramp` and `stations` are never empty. Non-canonical input is rejected, not
normalized: `parse_pack` requires `write_pack(parse(x)) == x`.

```text
{"schema":"salmong.scenario-pack","version":1,"gen":1,"pack_seed":1,"scenarios":[
{"id":0,"seed":3141592653,"w":64,"h":40,"ramp":[[0,40],[9000,90]],"stations":[[12,7,0,0,3],[30,22,1,0,5],[48,12,2,0,4]]}
]}
```

- `stations` rows are `[x, y, shape, at_tick, weight]` in spawn order. `ramp` rows are `[tick, rate]`, ascending from tick 0.
- `version` changes with the format; the loader accepts exactly 1. `gen` changes when a seed would give different data.
  Packs are regenerated, never migrated. Replay headers identify the pack by digest (section 3).
- Bend's limits: [no `U64`](https://github.com/bendlang/bend/blob/4024f04/README.md#L231), so both sides use a stateless `h32`, not core's PCG; [F32 printing differs by lane](https://github.com/bendlang/bend/blob/4024f04/WONTFIX.txt#L54), so no floats;
  [deep recursion overflows on JS](https://github.com/bendlang/bend/blob/4024f04/WONTFIX.txt#L147), so print line by line; [calls must be balanced](https://github.com/bendlang/bend/blob/4024f04/README.md#L240), so `count` is a power of two.

**Generator.** All arithmetic is wrapping `U32`. All constants below are *(tune)*, and changing one bumps `gen`.
```text
h32(s, i)      x = s ^ (i * 0x9E3779B1); x ^= x >> 16; x *= 0x85EBCA6B; x ^= x >> 13; x *= 0xC2B2AE35; x ^= x >> 16
r(p, k, a)     h32(seed, p << 24 | k << 8 | a), purpose p: 1 x, 2 y, 3 shape, 4 weight, 5 ramp. pick(lo,hi,v) = lo + v % (hi - lo + 1)
scenario id    seed = h32(pack_seed, id); w = 64; h = 40; ramp = [[0,40],[9000,pick(80,100,r(5,0,0))]]
station k<64   first a in 0..255 with x = pick(2,w-3,r(1,k,a)), y = pick(2,h-3,r(2,k,a)) and squared distance >= 16
               to every earlier station. No such a: the list ends at k; under 3 stations the generator exits non-zero
               shape = k if k < 3; else pick(0,2,r(3,k,0)) if at_tick < 8400; else pick(0,3,r(3,k,0))
               at_tick = 0 if k < 3, else (k - 2) * 600; weight = pick(3,5,r(4,k,0))
```

- **Demand (core only, not generated).** `rate(t)` is linear between `ramp` rows, truncating integer division on `u64`,
  last row held. Each tick an active station adds `weight * rate(t)` to its accumulator and spawns one passenger per
  `SPAWN_UNIT = 30000` *(tune)*. Arrival times are deterministic; PCG picks only `want`, uniformly over eligible shapes.
- **Where Rust reads it.** `salmong-game` embeds the file with `include_bytes!` and in `Loading` calls `salmong_pack::parse_pack`,
  which keeps serde out of core, rejects unknown fields and runs `Scenario::validate` per entry. `Sim::new` takes one `Scenario`.
- **Acceptance check (M4.4).** For seeds 1 to 3 and counts 1, 16, 64, `bend bend/scenarios.bend <seed> <count>` and `salmong-oracle
  pack <seed> <count>` must print `cmp`-equal bytes that pass `Scenario::validate`; seed 1 count 16 must match `pack.v1.sha256`.
  Equal bytes prove agreement only, so Rust mutations (wrong coordinate, hash index, ramp value) must each fail. CLI unverified until M4.1.

## 6. Web 60 fps budget

Frame budget is 16.67 ms. Bevy on wasm runs on one thread, so sim, ECS and render prep share the main thread. The GPU
overlaps it, so stages are guides, not a sum: the gate is measured frame interval.

| Stage (ordinary laptop, Chrome, Q3) | Target |
|---|---|
| Sim, one tick at peak | <= 2 ms |
| Mirror, interpolate, fill instances | <= 2 ms |
| Bevy extract, prepare, submit | <= 5 ms |
| Route recompute on edit (one-off, in wasm) | <= 2 ms |
| Worst main-thread frame: 2 ticks plus an edit | <= 13 ms |
| GPU, in parallel | <= 6 ms |

- **Population bound.** One overflow climb lets a station spawn `ceil(1350 * 5 * 100 / 30000) = 23`, so stations and full buses hold
  about `64 * (9 + 23) + 32 * 6 = 2,240`. Transfers can dodge timers, so the hard bound is the 5,000 spawn pause, and the bench targets it.
- **Entities:** 64 stations, 8 routes, 32 buses, under 200 with UI. A station draws at most 16 queued passengers *(tune)*
  plus a `+n` badge, so passenger instances stay under 1,300 at any population. Boarding picks are O(shapes) (section 2).
- **Batching:** passengers, stations and buses are one instanced draw each (vertex-rate instance buffers, shape chosen in
  the fragment shader), which keeps a later WebGL2 port possible. Route meshes rebuild on edit only. At most 20 draws per frame.
- **Fixed timestep:** 30 Hz via `Time<Fixed>`, 60 ticks/s at x2. A render frame runs at most 2 ticks; time beyond that is
  discarded, so the sim slows instead of spiralling. Hidden tab or pause: no ticks, accumulator cleared on return. Inputs
  are bound to tick numbers, so all of this changes when ticks run, never what they compute.

Measurement:
- **Protocol.** Stable Chrome, 1920x1080 at device-pixel ratio 1, mains power, no other tabs, on the laptop recorded at M0.3.
- `?bench=stress&seed=1` replays a fixed command log with a bench `Balance` that reaches 5,000 alive with large ineligible queues:
  30 s warm-up, then 120 s of steady load, repeated edits, x2 speed and 32 buses in service together. Three runs; the worst gates.
  JSON has device, browser, adapter, `requestAnimationFrame` interval p50, p95, p99 and frames over 20 ms. Pass: p95 <= 16.7 ms, p99 <= 20 ms.
  Deep dives use the in-game overlay and Chrome traces; native `cargo bench` on `step` and `solve` tracks core regressions.

## 7. Tooling gaps

| Need | Box today | Action | Blocks |
|---|---|---|---|
| Rust | Debian 1.85.1, no rustup, no wasm std | Install rustup, pin 1.97.1 (Bevy 0.20.1 MSRV), add `wasm32-unknown-unknown` | M0 |
| trunk, wasm-bindgen-cli | missing | `cargo install trunk --locked` (0.22.1, needs Rust 1.96.1; replaces wasm-pack); wasm-bindgen-cli at the `Cargo.lock` version (now 0.2.130) | M0 |
| wasm-opt, brotli | missing | Trunk may fetch `wasm-opt`; confirm at M0 | M5 |
| TLC | `tla2tools.jar` 2.19 in `/workspace/lim/.tools`, Java 17 | Set `TLA2TOOLS`; the jar belongs to another project, so copy or pin a hash | M1 |
| Bend 2 | missing; clang also missing (gcc is [not a target](https://github.com/bendlang/bend/blob/4024f04/WONTFIX.txt#L27)); Bun 1.4.2, Node 24 | Review `install.sh` before running, pin 2.0.39, set `BEND_NO_TELEMETRY=1`, use the JS target; clang only for M4.5 | M4 |
| GPU, browser | no `/dev/dri`; Chrome 156 WebGPU probe timed out, so this box cannot validate 60 fps | Ordinary laptop with Chrome for perf and WebGPU checks | M0, M5 |

## 8. Milestones

Each unit fits one change and has a check that passes or fails. Where: `box` runs here after M0, `gpu` needs the reference
laptop (Q3), `clang` needs clang 14+. M1 needs no new tools and precedes M2 to keep rule changes cheap. M4 needs only M2.3, so it can overlap M3.

| Unit | Work | Check | Where |
|---|---|---|---|
| M0.1 Rust | rustup, pin 1.97.1, wasm target | `rustc -V` is 1.97.1; empty lib passes `cargo check` for wasm32 | box |
| M0.2 Blank web build | Trunk, wasm-bindgen-cli, blank Bevy 2D app | `trunk build --release` exits 0; wasm size recorded; `cargo tree -e features` shows whether `webgpu` overrides the `webgl2` that `2d` pulls in | box |
| M0.3 WebGPU | Run the blank app on the reference laptop | App clears to a known colour and logs a WebGPU adapter; device recorded | gpu |
| M1.1 Safety | `Boarding.tla`, safety and fault configs | TLC completes the safety config; each `Fault` gives a counterexample on its property | box |
| M1.2 Liveness | Live config, disconnected final network included | TLC completes and passes `Delivery`, `BusProgress`, `NoPingPong` | box |
| M2.1 Pack format | `salmong-pack`: `parse_pack`, `write_pack`; `Scenario::validate` | Parse then write gives the same bytes; `version` 2, unknown fields, non-canonical bytes are errors; one bad fixture per rule returns that rule's error | box |
| M2.3 Rust reference | `h32`, generator, `salmong-oracle pack` | Golden `h32`, placement and ramp vectors pass; 1,024 seeds validate; seed 1 matches `pack.v1.sha256` | box |
| M2.4 Router | `network`, `DijkstraRouter`, `ReferenceRouter` | Exact tables, derived by hand first, on: disconnected, shared terminal, parallel routes, no path, reversal. Routers equal on proptest networks; a wrong hop cost fails | box |
| M2.5a Movement | `Sim`, shuttle, terminal reversal | Fixture route gives the expected stop sequence and arrival ticks | box |
| M2.5b Timed service | Timers, alight, board, bus order | Exact logs for competing buses, same-tick transfer, sustained arrivals; dwell <= free seats x 8 ticks | box |
| M2.5c Edits | Section 2 edit rules, `Retiring` | Fixture per bus phase, all original stops removed, reversed order, repeated edits while retiring; an invalid edit leaves `state_hash` unchanged | box |
| M2.5d Spawning | `spawn`, accumulator, spawn pause | Golden spawn ticks and `want` values; population never passes 5,000 | box |
| M2.5e Overflow | `overflow`, game over | Game over on exactly tick 1350 of overflow; decay of 2 per tick; safety proptests hold after every microstep | box |
| M2.6 Bot games | Greedy bot and do-nothing baseline | 10,000 ticks on all 16 scenarios: bot delivers at least 50% of spawned *(tune)* and outlives the baseline | box |
| M2.7 Determinism | wasm build of core, run in Node | Per-tick `state_hash` sequence equals native on fixed seeds; replay with a wrong header is refused | box |
| M2.8 Trace check | `BoardingTrace.tla` (needs M1.1) | TLC accepts traces from sample seeds; dropped, reordered or altered steps are rejected | box |
| M2.9 Perf workload | Bench `Balance`, `cargo bench` | Workload reaches 5,000 alive; peak `step` recorded before render work starts | box |
| M3.1 Render | Draw `SimView` from a replay | At equal tick counts, `state_hash` equals headless at several render rates | gpu |
| M3.2 Input | Pointer to `Command` | Scripted pointer events give the expected command log | box |
| M3.3 Web game | Browser build, placeholder art | A scripted game ends at the same tick, score and `state_hash` as headless | gpu |
| M3.4 Edit continuity | Network edits with riders on board | Scripted edits: no bus moves more than one tick's travel per tick, rider counts conserved. Playtest note is advisory | gpu |
| M4.1 Bend install | Review `install.sh`, pin 2.0.39 | `bend version` is 2.0.39; a hello program prints its line | box |
| M4.2 First scenario | Port `h32`; `bend/scenarios.bend` | M2.3 vectors pass on the JS lane; `bend/scenarios.bend 1 1` equals the Rust output byte for byte | box |
| M4.4 Pack check | Section 5 acceptance check | All seeds and counts `cmp` equal; every mutation fails | box |
| M4.5 Native lane | Native binary | Bytes equal the JS lane at 1 and 8 threads, count 1,024; times recorded, not gated | clang |
| M4.6 Bend go/no-go | Decide at M3.3 done or 5 working days after M4.1 starts, whichever is first. No M4.4 pass means no-go | No-go: with Bend absent, `salmong-oracle pack` regenerates the pack and the game loads it | box |
| M5.2 Frame gate | Instancing; `?bench=stress&seed=1` | At most 20 draw calls; section 6 protocol passes; stall, hidden tab, pause and x2 transitions keep the replay's `state_hash` | gpu |
| M5.3 Size | LTO, `opt-level`, `wasm-opt`, cost of `serde_json` | Download size meets the target set from the M0.2 baseline | box |
| M6.1 Weekly rewards | `rewards`, reward UI; per-route and per-station capacity in spec and core (needs Q7) | TLC and safety proptests pass with reward pause and mixed capacities; illegal `ChooseReward` is rejected | box |
| M6.2 Release | Original art and audio, tuning, static hosting | Every asset lists its author and licence; playtest sign-off; hosted URL reaches game over in a clean browser profile | gpu |

## 9. Open questions

Q1 to Q6 are **DECIDED; LEJ may change them.**
- **Q1. Is "routing in Bend" a hard product claim? DECIDED: no.** Bend does offline scenario generation only, and that
  is enough. Rust does all routing. The Bend router oracle is dropped; `ReferenceRouter` keeps a second implementation.
- **Q2. WebGL2 fallback? DECIDED: none in MVP.** Browsers without WebGPU get the error screen.
- **Q3. Reference perf machine? DECIDED: ordinary laptop, Chrome.** Section 6 fixes the run protocol.
- **Q4. Loop routes in MVP? DECIDED: after MVP.** They change `Dir`, terminals and the spec.
- **Q5. Bend 2 or Bend 1? DECIDED: Bend 2.**
- **Q6. Tick rate? DECIDED: 30 Hz with interpolation.**
- **Q7. First release scope? OPEN.** Are weekly rewards in? Local best score only, or seeded daily challenges too?

## 10. Review findings

**Done.** Codex reviewed the draft (`/workspace/salmong-artifacts/plan-review-codex.md`); all 18 findings are applied above, 4 in part. Main
gaps: undefined semantics and formal checks that could not fail. Q1 settles Bend placement; the core and game split held once core owned mode and ticks.

| # | Severity | Decision | Reason |
|---|---|---|---|
| 1 | high | Accept | Section 2 now gives edges, `INF` arithmetic, boundary values and exact equations; M2.4 tables are hand-derived first |
| 2 | high | Accept | Reversal is alight plus reboard in the graph; potential over `Wait`/`Ride` replaces `cost[station]` |
| 3 | high | Accept | Moving buses own their segment, `Retiring` state added, per-phase rules, atomic rejection, M2.5c fixtures |
| 4 | high | Accept (part) | Per-bus timers and stable bus order added. No snapshot or max dwell: seats already bound boarding, so M2.5b tests that |
| 5 | high | Accept | `Delivery` restated for the final topology, fairness per bus, game over ends the guarantee |
| 6 | high | Accept | Spec models `network`, `dir`, `retiring`; `Table(network)` is derived; `Networks` includes disconnected cases |
| 7 | med | Accept | `Capacity + 1` could never fail; `Fault` actions added, transition rules are action properties, exploration reported |
| 8 | high | Accept | Separate microstep trace with `PassengerId` and a TLA+ mapping; corrupted traces must fail |
| 9 | med | Accept (part) | Table `rev` and output contract added. No `Sim` injection or subprocess errors: Q1 removes the Bend oracle |
| 10 | high | Accept | Rules version, pack digest, `Balance`, ordered validation at execution, canonical `state_bytes` |
| 11 | high | Accept | Generator fully specified with exhaustion behaviour; example now has 3 stations; golden vectors at M2.3 |
| 12 | med | Accept | Byte grammar, strict parser, several seeds and counts, validation of every pack, semantic mutations |
| 13 | high | Accept | Bound derived; transfers defeat it, so a 5,000 spawn pause enforces it. Per-shape FIFOs, capped drawing, M2.9 workload |
| 14 | high | Accept | Budget now uses peak ticks; at most 2 ticks per frame; stall, hidden-tab, pause and x2 behaviour defined and tested |
| 15 | high | Accept (part) | Protocol, viewport, runs and interval gate fixed. Q3 sets the class, so the laptop is recorded, not specified in advance |
| 16 | med | Accept | `SimMode` authoritative, inputs bound to ticks, previous/current snapshots, equal-hash check at M3.1 |
| 17 | med | Accept | M2.5 split into five units; objective checks at M2.6, M3.3, M3.4; workload at M2.9. M4.6's bundle went with Q1 |
| 18 | med | Accept (part) | M6.1 extends capacities, pause and legality; M4.6 go/no-go added. Trigger is event-based, as the plan has no calendar |
