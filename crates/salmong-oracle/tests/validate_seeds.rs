use salmong_oracle::generate_pack;
use salmong_pack::parse_pack;

#[test]
fn one_thousand_twenty_four_seeds_validate() {
    for pack_seed in 0u32..1024 {
        let pack = generate_pack(pack_seed, 1)
            .unwrap_or_else(|error| panic!("pack_seed {pack_seed} failed to generate: {error:?}"));
        let bytes = salmong_pack::write_pack(&pack);
        parse_pack(&bytes).unwrap_or_else(|error| {
            panic!("pack_seed {pack_seed} failed validate/parse: {error:?}")
        });
    }
}
