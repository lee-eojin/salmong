# Assets

Every file under `assets/` and every shader file has one row here. `scripts/check-assets.sh` fails if a file has no row, a row has no author or licence, or a row names a file that is not in the repository.

| Path | What | Author | Licence | Made by |
|---|---|---|---|---|
| `crates/salmong-game/src/shapes.wesl` | Shader art: shapes and colours in the fragment shader | The salmong authors | MIT OR Apache-2.0 | Hand-written |
| `assets/scenarios/pack.v1.json` | Scenario pack, seeds 1 to 16 | The salmong authors | MIT OR Apache-2.0 | `salmong-oracle pack 1 16` |
| `assets/scenarios/pack.v1.sha256` | sha256 of `pack.v1.json` | The salmong authors | MIT OR Apache-2.0 | `salmong-oracle pack 1 16` |
