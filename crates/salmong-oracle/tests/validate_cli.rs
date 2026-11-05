use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn pack_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/scenarios/pack.v1.json")
}

fn oracle(args: &[&str]) -> (Option<i32>, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_salmong-oracle"))
        .args(args)
        .output()
        .unwrap();
    (
        out.status.code(),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

#[test]
fn committed_pack_validates() {
    let path = pack_path();
    let got = oracle(&["validate", path.to_str().unwrap()]);
    assert_eq!(
        got,
        (Some(0), "ok: 16 scenarios\n".to_string(), String::new())
    );
}

#[test]
fn out_of_bounds_station_is_refused() {
    let text = fs::read_to_string(pack_path()).unwrap();
    let bad = text.replacen("[[35,32,0,0,4]", "[[70,32,0,0,4]", 1);
    assert_ne!(bad, text);
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("validate_out_of_bounds.json");
    fs::write(&path, bad).unwrap();
    let (code, out, err) = oracle(&["validate", path.to_str().unwrap()]);
    assert_eq!(code, Some(1));
    assert_eq!(out, "");
    assert!(err.contains("OutOfBounds"), "{err}");
}

#[test]
fn missing_file_is_refused() {
    let (code, out, err) = oracle(&["validate", "/nonexistent/pack.json"]);
    assert_eq!(code, Some(1));
    assert_eq!(out, "");
    assert!(err.starts_with("/nonexistent/pack.json"), "{err}");
}

#[test]
fn wrong_argument_count_prints_usage() {
    for args in [&["validate"][..], &["validate", "a", "b"]] {
        let (code, out, err) = oracle(args);
        assert_eq!(code, Some(2), "{args:?}");
        assert_eq!(out, "", "{args:?}");
        assert!(
            err.contains("usage: salmong-oracle validate <file>"),
            "{err}"
        );
    }
}
