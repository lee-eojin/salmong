# salmong

salmong is a bus-route puzzle game for the browser, in the same genre as Mini Metro. You drag routes between stations, and buses carry each passenger to the station shape that passenger wants. The game ends when a station stays overcrowded for too long.

salmong shares mechanics only. Its names, art, audio, UI look, and maps are its own.

## Status

salmong is in milestone M0, so there is no game to play yet. The app opens a blank window and clears it to salmon, `#FA8072`. Nobody has run the M0.3 check on a real GPU yet, so [the M0.3 procedure](docs/m0.3-webgpu-check.md) still has the status `NOT_RUN`.

## Stack

| Part | Built with | State |
|---|---|---|
| Game client | Rust and [Bevy](https://bevy.org) 0.20.1, compiled to WASM and rendered with WebGPU | Blank app in `crates/salmong-game` |
| Rules and routing | Rust in `crates/salmong-core`, with integer maths and no Bevy | Scenario types, validation, network snapshot and routing table; sim rules planned for M2.5 |
| Scenarios | [Bend 2](https://github.com/bendlang/bend), run offline. Bend writes the scenario pack that the game loads, and it never runs in the browser | Pack reader and writer in `crates/salmong-pack`; Bend 2.0.39 pinned and checked on the box (M4.1); `bend/scenarios.bend` writes the same bytes as `salmong-oracle pack` on the JS lane (M4.2) |
| Rule checks | [TLA+](https://github.com/tlaplus/tlaplus). TLC checks the boarding rules before they are coded | Safety and liveness models in `spec/` |

WebGPU is the only web backend in the MVP. A browser with no WebGPU adapter shows an error screen.

[The design plan](docs/plan.md) has the rules, the decisions, and [the milestones](docs/plan.md#8-milestones).

## Build and run

Install these tools first. [Before you start](docs/m0.3-webgpu-check.md#before-you-start) has the install commands for Trunk and wasm-bindgen-cli.

| Tool | Version |
|---|---|
| Rust | 1.99.1 on the `stable` channel, with the `wasm32-unknown-unknown` target |
| Trunk | 0.22.1 |
| wasm-bindgen-cli | 0.2.130, which must match `Cargo.lock` |
| Node.js | 24 |
| TLC and Java | TLC 2.19 (08 August 2024, revision `5a47802`), Java 17; set `TLA2TOOLS` to the jar or provide `tlc` on PATH |
| Bend | 2.0.39, from `scripts/install-bend.sh`; only `scripts/check-bend.sh` and `scripts/check-scenarios.sh` need it |

Run every command from the repository root.

To run the tests:

```sh
cargo test --locked --workspace
```

To build the web app into `dist/`:

```sh
trunk build --release
```

To run the app, start the development server:

```sh
trunk serve --release
```

Wait for `server listening`, then open `http://localhost:8080/` in a browser that supports WebGPU. The first build takes several minutes.

## Check the repository

These scripts exit 0 when they pass:

```sh
scripts/check-toolchain.sh
scripts/check-web-build.sh
scripts/check-no-comments.sh
scripts/check-boarding.sh
scripts/check-trace.sh
cargo bench --locked -p salmong-core --bench step
scripts/check-bend.sh
scripts/check-scenarios.sh
```

- `scripts/check-toolchain.sh` fails if `rustc` is not 1.99.1, and then runs `cargo check` for wasm32.
- `scripts/check-web-build.sh` builds the release wasm and confirms that WebGPU is the only web backend.
- `scripts/check-boarding.sh` fails on a different TLC build, a safety or liveness violation or deadlock, incomplete exploration, mismatched fault names, or a deliberate fault that does not break its expected property. Use `--simulate` for the sandbox self-check; it is not exhaustive.
- `scripts/check-trace.sh` runs `salmong-oracle trace` on seeds 1 to 4 and checks each trace with `spec/BoardingTrace.tla`. It fails on a different TLC build, a sample trace TLC does not accept in full, or a dropped, swapped or altered trace that TLC does not reject with a deadlock. It needs `TLA2TOOLS` or `tlc`, like `check-boarding.sh`.
- `cargo bench -p salmong-core --bench step` runs the M2.9 workload to 5,000 alive and prints peak `step` and `solve` times. It fails if the cap is missed. The record is in [docs/m2.9-perf.md](docs/m2.9-perf.md).
- `scripts/check-bend.sh` fails unless `bend` on PATH is the shim from `scripts/install-bend.sh`, prints `bend 2.0.39` and matches its pinned sha256, and `bend/hello.bend` prints its line with telemetry off and no network. [docs/m4.1-bend-install.md](docs/m4.1-bend-install.md) has the install review.
- `scripts/check-scenarios.sh` fails unless `bend/vectors.bend` passes the M2.3 vectors, `bend/scenarios.bend` prints the same bytes as `salmong-oracle pack` for seeds 1, 0 and 4294967295 at count 1 and matches `assets/scenarios/pack.v1.json` at `1 16`, bad arguments exit 2 with empty stdout, and 16 mutants of `bend/scenarios.bend` fail as designed.
- `scripts/check-no-comments.sh` fails if a source file holds a code comment. `cargo test` runs the same check.
- `scripts/check-mutants.sh [base]` runs cargo-mutants on the `.rs` lines changed since `base` (default `origin/main`, or `--all` for every crate but salmong-game) and fails on a missed mutant that `mutants-allow.toml` does not list with a reason. It counts a timeout as caught and skips when no `.rs` file changed. CI runs it on every pull request.
- `scripts/check-unit-prs.sh` fails if an open PR's head is not `unit/<unit>` or two open PRs share one unit. CI runs it on every pull request.

Before you start a milestone unit, run `scripts/claim-unit.sh <unit>`. It pushes `unit/<unit>` from `origin/main` and fails if that branch or an open PR for it already exists, so only one worker holds a unit.

To confirm that this README is still true, run `scripts/check-readme.sh`. It runs every command above and follows every link.
