use std::path::PathBuf;
use std::process::Command;

use salmong_oracle::pack_bytes;
use salmong_pack::parse_pack;

fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/scenarios")
}

#[test]
fn seed_one_count_sixteen_matches_committed_pack() {
    let bytes = pack_bytes(1, 16).expect("seed 1 count 16 generates");
    parse_pack(&bytes).expect("generated pack parses");
    let json_path = assets().join("pack.v1.json");
    let sha_path = assets().join("pack.v1.sha256");
    let committed = std::fs::read(&json_path).expect("pack.v1.json exists");
    assert_eq!(
        bytes, committed,
        "CLI bytes must match assets/scenarios/pack.v1.json"
    );
    let digest = Command::new("sha256sum")
        .arg(&json_path)
        .output()
        .expect("sha256sum runs");
    assert!(digest.status.success());
    let hex = String::from_utf8(digest.stdout).unwrap();
    let hex = hex.split_whitespace().next().expect("sha256 hex");
    let pinned = std::fs::read_to_string(&sha_path).expect("pack.v1.sha256 exists");
    assert_eq!(pinned.trim(), hex);
}
