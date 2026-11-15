# Assets

Every file under `assets/` and every shader file has one row here. `scripts/check-assets.sh` fails if a file has no row, a row has no author or licence, or a row names a file that is not in the repository.

| Path | What | Author | Licence | Made by |
|---|---|---|---|---|
| `crates/salmong-game/src/shapes.wesl` | Shader art: shapes and colours in the fragment shader | The salmong authors | MIT OR Apache-2.0 | Hand-written |
| `assets/scenarios/pack.v1.json` | Scenario pack, seeds 1 to 16 | The salmong authors | MIT OR Apache-2.0 | `salmong-oracle pack 1 16` |
| `assets/scenarios/pack.v1.sha256` | sha256 of `pack.v1.json` | The salmong authors | MIT OR Apache-2.0 | `salmong-oracle pack 1 16` |
| `assets/sounds/board.wav` | Boarded cue, triangle 440 Hz, 1,102 samples | The salmong authors | MIT OR Apache-2.0 | `scripts/make-sounds.mjs` |
| `assets/sounds/deliver.wav` | Delivered cue, triangle 660 then 880 Hz, 2,646 samples | The salmong authors | MIT OR Apache-2.0 | `scripts/make-sounds.mjs` |
| `assets/sounds/station.wav` | Station spawned cue, triangle 523, 659 then 784 Hz, 6,615 samples | The salmong authors | MIT OR Apache-2.0 | `scripts/make-sounds.mjs` |
| `assets/sounds/overflow.wav` | Overflow started cue, square 196 Hz, two pulses, 6,615 samples | The salmong authors | MIT OR Apache-2.0 | `scripts/make-sounds.mjs` |
| `assets/sounds/reject.wav` | Command rejected cue, square 147 Hz, 3,307 samples | The salmong authors | MIT OR Apache-2.0 | `scripts/make-sounds.mjs` |
| `assets/sounds/over.wav` | Game over cue, triangle 392, 330, 262 then 196 Hz, 19,845 samples | The salmong authors | MIT OR Apache-2.0 | `scripts/make-sounds.mjs` |
