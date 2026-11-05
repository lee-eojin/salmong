#![cfg(not(target_arch = "wasm32"))]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Syntax {
    Bend,
    Css,
    GitIgnore,
    Html,
    JavaScript,
    Rust,
    Shell,
    Tla,
    Toml,
    Yaml,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FileClass {
    Scan(Syntax),
    Exempt,
}

const BY_NAME: &[(&str, FileClass)] = &[
    ("Cargo.lock", FileClass::Exempt),
    (".gitignore", FileClass::Scan(Syntax::GitIgnore)),
];

const BY_EXTENSION: &[(&str, FileClass)] = &[
    ("bend", FileClass::Scan(Syntax::Bend)),
    ("cfg", FileClass::Scan(Syntax::Tla)),
    ("css", FileClass::Scan(Syntax::Css)),
    ("html", FileClass::Scan(Syntax::Html)),
    ("js", FileClass::Scan(Syntax::JavaScript)),
    ("json", FileClass::Exempt),
    ("md", FileClass::Exempt),
    ("mjs", FileClass::Scan(Syntax::JavaScript)),
    ("rs", FileClass::Scan(Syntax::Rust)),
    ("sh", FileClass::Scan(Syntax::Shell)),
    ("sha256", FileClass::Exempt),
    ("tla", FileClass::Scan(Syntax::Tla)),
    ("toml", FileClass::Scan(Syntax::Toml)),
    ("txt", FileClass::Exempt),
    ("yml", FileClass::Scan(Syntax::Yaml)),
];

fn classify(path: &str) -> Option<FileClass> {
    let path = Path::new(path);
    let lookup = |table: &[(&str, FileClass)], key: Option<&str>| {
        table.iter().find(|row| Some(row.0) == key).map(|row| row.1)
    };
    lookup(BY_NAME, path.file_name().and_then(|name| name.to_str()))
        .or_else(|| lookup(BY_EXTENSION, path.extension().and_then(|ext| ext.to_str())))
}

#[derive(Clone, Copy)]
enum LineMarker {
    None,
    Anywhere(&'static str),
    WordStart(&'static str),
    LineStart(&'static str),
}

#[derive(Clone, Copy)]
struct Grammar {
    line: LineMarker,
    block: Option<(&'static str, &'static str)>,
    nested: bool,
    quotes: &'static [(&'static str, bool)],
    rust_literals: bool,
}

const fn grammar(syntax: Syntax) -> Grammar {
    let plain = Grammar {
        line: LineMarker::None,
        block: None,
        nested: false,
        quotes: &[],
        rust_literals: false,
    };
    match syntax {
        Syntax::Bend => Grammar {
            line: LineMarker::Anywhere("#"),
            quotes: &[("\"", true)],
            ..plain
        },
        Syntax::Css => Grammar {
            block: Some(("/*", "*/")),
            quotes: &[("\"", true), ("'", true)],
            ..plain
        },
        Syntax::GitIgnore => Grammar {
            line: LineMarker::LineStart("#"),
            ..plain
        },
        Syntax::Rust => Grammar {
            line: LineMarker::Anywhere("//"),
            block: Some(("/*", "*/")),
            nested: true,
            rust_literals: true,
            ..plain
        },
        Syntax::Shell | Syntax::Yaml => Grammar {
            line: LineMarker::WordStart("#"),
            quotes: &[("'", false), ("\"", true)],
            ..plain
        },
        Syntax::Tla => Grammar {
            line: LineMarker::Anywhere("\\*"),
            block: Some(("(*", "*)")),
            nested: true,
            quotes: &[("\"", true)],
            ..plain
        },
        Syntax::Toml => Grammar {
            line: LineMarker::Anywhere("#"),
            quotes: &[("\"\"\"", true), ("'''", false), ("\"", true), ("'", false)],
            ..plain
        },
        Syntax::Html | Syntax::JavaScript => plain,
    }
}

fn starts(source: &[u8], at: usize, marker: &str) -> bool {
    source[at..].starts_with(marker.as_bytes())
}

fn find(source: &[u8], from: usize, marker: &str) -> Option<usize> {
    source[from.min(source.len())..]
        .windows(marker.len())
        .position(|window| window == marker.as_bytes())
        .map(|found| from + found)
}

fn line_end(source: &[u8], from: usize) -> usize {
    find(source, from, "\n").unwrap_or(source.len())
}

fn skip_quoted(source: &[u8], open: usize, delimiter: &str, escapes: bool) -> usize {
    let mut at = open + delimiter.len();
    while at < source.len() {
        if escapes && source[at] == b'\\' {
            at += 2;
        } else if starts(source, at, delimiter) {
            return at + delimiter.len();
        } else {
            at += 1;
        }
    }
    source.len()
}

fn skip_block(source: &[u8], open: usize, marks: (&str, &str), nested: bool) -> usize {
    let mut at = open + marks.0.len();
    let mut depth = 1;
    while at < source.len() {
        if nested && starts(source, at, marks.0) {
            depth += 1;
            at += marks.0.len();
        } else if starts(source, at, marks.1) {
            depth -= 1;
            at += marks.1.len();
            if depth == 0 {
                return at;
            }
        } else {
            at += 1;
        }
    }
    source.len()
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn skip_rust_literal(source: &[u8], at: usize) -> Option<usize> {
    let previous_is_identifier = at > 0 && is_identifier_byte(source[at - 1]);
    match source[at] {
        b'"' => Some(skip_quoted(source, at, "\"", true)),
        b'\'' if source.get(at + 1) == Some(&b'\\') => Some(skip_quoted(source, at, "'", true)),
        b'\'' => {
            let width = match source.get(at + 1)? {
                0..=0x7F => 1,
                0xC0..=0xDF => 2,
                0xE0..=0xEF => 3,
                _ => 4,
            };
            (source.get(at + 1 + width) == Some(&b'\'')).then_some(at + 2 + width)
        }
        b'r' if !previous_is_identifier => {
            let hashes = source[at + 1..]
                .iter()
                .take_while(|byte| **byte == b'#')
                .count();
            let open = at + 1 + hashes;
            if source.get(open) != Some(&b'"') {
                return None;
            }
            let close = format!("\"{}", "#".repeat(hashes));
            Some(find(source, open + 1, &close).map_or(source.len(), |end| end + close.len()))
        }
        _ => None,
    }
}

fn scan(grammar: Grammar, source: &[u8], start: usize, found: &mut Vec<usize>) {
    let mut at = start;
    'bytes: while at < source.len() {
        if grammar.rust_literals {
            if let Some(end) = skip_rust_literal(source, at) {
                at = end;
                continue;
            }
        }
        for (delimiter, escapes) in grammar.quotes {
            if starts(source, at, delimiter) {
                at = skip_quoted(source, at, delimiter, *escapes);
                continue 'bytes;
            }
        }
        let line_comment = match grammar.line {
            LineMarker::None => false,
            LineMarker::Anywhere(marker) => starts(source, at, marker),
            LineMarker::WordStart(marker) => {
                starts(source, at, marker) && (at == 0 || b" \t\n;&|()".contains(&source[at - 1]))
            }
            LineMarker::LineStart(marker) => {
                starts(source, at, marker) && (at == 0 || source[at - 1] == b'\n')
            }
        };
        if line_comment {
            found.push(at);
            at = line_end(source, at);
            continue;
        }
        if let Some(marks) = grammar.block {
            if starts(source, at, marks.0) {
                found.push(at);
                at = skip_block(source, at, marks, grammar.nested);
                continue;
            }
        }
        at += 1;
    }
}

fn regex_may_start(previous: Option<u8>) -> bool {
    previous.is_none_or(|byte| b"(,=:[!&|?{};+-*%<>~^".contains(&byte))
}

fn skip_regex(source: &[u8], open: usize) -> Option<usize> {
    let mut at = open + 1;
    let mut in_class = false;
    while at < source.len() {
        match source[at] {
            b'\n' => return None,
            b'\\' => at += 1,
            b'[' => in_class = true,
            b']' => in_class = false,
            b'/' if !in_class => return Some(at + 1),
            _ => {}
        }
        at += 1;
    }
    None
}

fn skip_template(source: &[u8], open: usize, found: &mut Vec<usize>) -> usize {
    let mut at = open + 1;
    while at < source.len() {
        if source[at] == b'\\' {
            at += 2;
        } else if source[at] == b'`' {
            return at + 1;
        } else if starts(source, at, "${") {
            at = scan_javascript(source, at + 2, true, found);
        } else {
            at += 1;
        }
    }
    source.len()
}

fn scan_javascript(
    source: &[u8],
    start: usize,
    in_template: bool,
    found: &mut Vec<usize>,
) -> usize {
    let mut at = start;
    let mut depth = 0;
    let mut previous = None;
    while at < source.len() {
        let byte = source[at];
        if byte.is_ascii_whitespace() {
            at += 1;
            continue;
        }
        if starts(source, at, "//") {
            found.push(at);
            at = line_end(source, at);
            continue;
        }
        if starts(source, at, "/*") {
            found.push(at);
            at = skip_block(source, at, ("/*", "*/"), false);
            continue;
        }
        at = match byte {
            b'"' | b'\'' => skip_quoted(source, at, if byte == b'"' { "\"" } else { "'" }, true),
            b'`' => skip_template(source, at, found),
            b'/' if regex_may_start(previous) => skip_regex(source, at).unwrap_or(at + 1),
            b'{' => {
                depth += 1;
                at + 1
            }
            b'}' if in_template && depth == 0 => return at + 1,
            b'}' => {
                depth -= 1;
                at + 1
            }
            _ => at + 1,
        };
        previous = Some(byte);
    }
    source.len()
}

fn embedded(
    source: &[u8],
    tag_at: usize,
    tag: &str,
    syntax: Syntax,
    found: &mut Vec<usize>,
) -> usize {
    let close_tag = format!("</{tag}");
    let body = find(source, tag_at, ">").map_or(source.len(), |end| end + 1);
    let close = find(source, body, &close_tag).unwrap_or(source.len());
    found.extend(
        comment_offsets(syntax, &source[body..close])
            .into_iter()
            .map(|offset| body + offset),
    );
    close
}

fn scan_html(source: &[u8], found: &mut Vec<usize>) {
    let mut at = 0;
    while at < source.len() {
        if starts(source, at, "<!--") {
            found.push(at);
            at = find(source, at, "-->").map_or(source.len(), |end| end + 3);
        } else if starts(source, at, "<script") {
            at = embedded(source, at, "script", Syntax::JavaScript, found);
        } else if starts(source, at, "<style") {
            at = embedded(source, at, "style", Syntax::Css, found);
        } else {
            at += 1;
        }
    }
}

fn comment_offsets(syntax: Syntax, source: &[u8]) -> Vec<usize> {
    let mut found = Vec::new();
    let body = if matches!(syntax, Syntax::Shell | Syntax::JavaScript) && source.starts_with(b"#!")
    {
        line_end(source, 0)
    } else {
        0
    };
    match syntax {
        Syntax::Html => scan_html(source, &mut found),
        Syntax::JavaScript => {
            scan_javascript(source, body, false, &mut found);
        }
        _ => scan(grammar(syntax), source, body, &mut found),
    }
    found
}

fn comment_lines(syntax: Syntax, source: &str) -> Vec<usize> {
    let bytes = source.as_bytes();
    let mut lines: Vec<usize> = comment_offsets(syntax, bytes)
        .into_iter()
        .map(|offset| {
            1 + bytes[..offset]
                .iter()
                .filter(|byte| **byte == b'\n')
                .count()
        })
        .collect();
    lines.sort_unstable();
    lines.dedup();
    lines
}

fn repo_root() -> PathBuf {
    let manifest_dir = std::env::var_os("CARGO_MANIFEST_DIR")
        .expect("CARGO_MANIFEST_DIR is unset, so run this test through cargo test");
    Path::new(&manifest_dir).join("../..")
}

fn listed_files() -> Vec<String> {
    let output = Command::new("git")
        .args([
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ])
        .current_dir(repo_root())
        .output()
        .expect("git must be on PATH");
    assert!(output.status.success(), "git ls-files failed: {output:?}");
    String::from_utf8(output.stdout)
        .expect("tracked paths are UTF-8")
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(String::from)
        .collect()
}

fn violations(path: &str) -> Vec<String> {
    let full = repo_root().join(path);
    if !full.exists() {
        return Vec::new();
    }
    match classify(path) {
        None => vec![format!(
            "{path}: no syntax row, add one to BY_NAME or BY_EXTENSION"
        )],
        Some(FileClass::Exempt) => Vec::new(),
        Some(FileClass::Scan(syntax)) => {
            let source = std::fs::read_to_string(&full).expect("scanned files are UTF-8");
            comment_lines(syntax, &source)
                .into_iter()
                .map(|line| format!("{path}:{line}: comment"))
                .collect()
        }
    }
}

#[test]
fn tracked_source_has_no_comments() {
    let files = listed_files();
    assert!(
        files.iter().any(|path| path == "Cargo.toml"),
        "the file list is missing Cargo.toml: {files:?}"
    );
    let failures: Vec<String> = files.iter().flat_map(|path| violations(path)).collect();
    assert!(failures.is_empty(), "FAIL:\n{}", failures.join("\n"));
}

#[test]
fn finds_comments_in_every_syntax() {
    let cases: &[(Syntax, &str, &[usize])] = &[
        (Syntax::Rust, "fn a() {}\n// b\n", &[2]),
        (Syntax::Rust, "/// a\nfn b() {}\n", &[1]),
        (Syntax::Rust, "//! a\n", &[1]),
        (Syntax::Rust, "fn a() {} /* b /* c */ d */ // e\n", &[1]),
        (Syntax::Rust, "/* a /* b */ c */\n// d\n", &[1, 2]),
        (Syntax::Rust, "let a = 'x'; // b\n", &[1]),
        (Syntax::Rust, "fn a<'b>(c: &'b str) {} // d\n", &[1]),
        (Syntax::Toml, "a = 1 # b\n", &[1]),
        (Syntax::Toml, "# a\nb = 1\n", &[1]),
        (Syntax::Shell, "# a\necho b # c\n", &[1, 2]),
        (Syntax::Shell, "#!/bin/sh\n# a\n", &[2]),
        (Syntax::Shell, "a; # b\n", &[1]),
        (Syntax::JavaScript, "#!/usr/bin/env node\n// a\n", &[2]),
        (Syntax::JavaScript, "a(); /* b */\n", &[1]),
        (Syntax::JavaScript, "`${a /* b */}`;\n", &[1]),
        (Syntax::JavaScript, "a = b / c; // d\n", &[1]),
        (Syntax::Css, "a { b: c; } /* d */\n", &[1]),
        (Syntax::Html, "<p>a</p>\n<!-- b -->\n", &[2]),
        (Syntax::Html, "<style>\n/* a */\n</style>\n", &[2]),
        (
            Syntax::Html,
            "<script type=\"module\">\n// a\n</script>\n",
            &[2],
        ),
        (Syntax::Tla, "A == 1 \\* b\n", &[1]),
        (Syntax::Tla, "Next == x' = 1 \\* b\n", &[1]),
        (Syntax::Tla, "(* a (* b *) c *)\n(* d *)\n", &[1, 2]),
        (Syntax::Bend, "# a\ndef b() -> U32:\n  1 # c\n", &[1, 3]),
        (Syntax::GitIgnore, "# a\n/b\n", &[1]),
    ];
    for (syntax, source, expected) in cases {
        assert_eq!(
            comment_lines(*syntax, source),
            *expected,
            "{syntax:?}: {source:?}"
        );
    }
}

#[test]
fn ignores_comment_markers_that_are_not_comments() {
    let cases: &[(Syntax, &str)] = &[
        (Syntax::Rust, "#![cfg(test)]\n#[test]\nfn a() {}\n"),
        (Syntax::Rust, "let a = \"http://b /* c\";\n"),
        (Syntax::Rust, "let a = r#\"// \" /* \"#;\n"),
        (Syntax::Rust, "let a = '\"'; let b = \"c\";\n"),
        (Syntax::Rust, "let a = '/'; let b = '\\''; let c = 'é';\n"),
        (Syntax::Rust, "fn a<'b>() {} let c = \"//\";\n"),
        (Syntax::Rust, "let r#type = 1;\n"),
        (
            Syntax::Toml,
            "a = \"#b\"\nc = '#d'\ne = \"\"\"\n#f\n\"\"\"\n",
        ),
        (Syntax::Shell, "#!/usr/bin/env bash\n"),
        (Syntax::Shell, "echo ${#a[@]} $# a#b '#c' \"#d\"\n"),
        (Syntax::Shell, "grep -q 'a \"b\"' <<<\"$c\"\n"),
        (Syntax::JavaScript, "#!/usr/bin/env node\n"),
        (Syntax::JavaScript, "a.match(/b:\\/\\/[^:]+:(\\d+)\\//);\n"),
        (Syntax::JavaScript, "a(`http://${b}:${c}/`);\n"),
        (Syntax::JavaScript, "a(`b${`//${c}`}d`);\n"),
        (Syntax::JavaScript, "a(\"http://b\", 'c//d');\n"),
        (Syntax::JavaScript, "a = /[/]/;\n"),
        (Syntax::Css, "a { b: url(\"/*c*/\"); }\n"),
        (
            Syntax::Html,
            "<!doctype html>\n<a href=\"https://b/#c\">d</a>\n",
        ),
        (Syntax::Html, "<script>a(\"<!-- b -->\");</script>\n"),
        (
            Syntax::Tla,
            "---- MODULE A ----\nB == \"\\*\"\nC == 1 * 2\n====\n",
        ),
        (Syntax::Tla, "Next == x' = x + 1 /\\ y' = y \\\\ {z}\n"),
        (Syntax::Bend, "def a() -> String:\n  \"#b\"\n"),
        (Syntax::GitIgnore, "/a#b\n"),
    ];
    for (syntax, source) in cases {
        assert_eq!(
            comment_lines(*syntax, source),
            Vec::<usize>::new(),
            "{syntax:?}: {source:?}"
        );
    }
}

#[test]
fn classifies_files() {
    assert_eq!(classify("README.md"), Some(FileClass::Exempt));
    assert_eq!(classify("Cargo.lock"), Some(FileClass::Exempt));
    assert_eq!(
        classify("assets/scenarios/pack.v1.json"),
        Some(FileClass::Exempt)
    );
    assert_eq!(
        classify("spec/Boarding.safety.cfg"),
        Some(FileClass::Scan(Syntax::Tla))
    );
    assert_eq!(classify("assets/unknown.wgsl"), None);
    assert_eq!(classify("Makefile"), None);
    assert_eq!(
        classify(".github/workflows/ci.yml"),
        Some(FileClass::Scan(Syntax::Yaml))
    );
}

fn scan_in_child(manifest_dir: Option<&Path>) -> Output {
    let mut scan = Command::new(std::env::current_exe().expect("the test binary has a path"));
    scan.args(["--exact", "tracked_source_has_no_comments"]);
    scan.env_remove("RUST_TEST_NOCAPTURE");
    match manifest_dir {
        Some(dir) => scan.env("CARGO_MANIFEST_DIR", dir),
        None => scan.env_remove("CARGO_MANIFEST_DIR"),
    };
    scan.output().expect("the test binary starts")
}

fn fixture_tree_with_a_comment() -> PathBuf {
    let root = std::env::temp_dir().join(format!("salmong-no-comments-{}", std::process::id()));
    std::fs::create_dir_all(root.join("crates/salmong-core")).expect("fixture dirs are writable");
    std::fs::create_dir_all(root.join("scripts")).expect("fixture dirs are writable");
    std::fs::write(root.join("Cargo.toml"), "[workspace]\n").expect("fixture files are writable");
    std::fs::write(root.join("scripts/planted.sh"), "# planted\n")
        .expect("fixture files are writable");
    let init = Command::new("git")
        .args(["init", "-q"])
        .current_dir(&root)
        .status()
        .expect("git must be on PATH");
    assert!(init.success(), "git init failed in {root:?}");
    root
}

#[test]
fn scan_reads_the_tree_named_at_run_time() {
    let root = fixture_tree_with_a_comment();
    let output =
        std::panic::catch_unwind(|| scan_in_child(Some(&root.join("crates/salmong-core"))));
    std::fs::remove_dir_all(&root).expect("the fixture tree is removable");
    let output = output.unwrap_or_else(|panic| std::panic::resume_unwind(panic));
    let report = String::from_utf8_lossy(&output.stdout);
    assert!(
        !output.status.success(),
        "the scan missed the fixture comment: {report}"
    );
    assert!(
        report.contains("scripts/planted.sh:1: comment"),
        "wrong report: {report}"
    );
}

#[test]
fn scan_fails_without_a_run_time_manifest_dir() {
    let output = scan_in_child(None);
    let report = String::from_utf8_lossy(&output.stdout);
    assert!(
        !output.status.success(),
        "the scan ran with no run-time root: {report}"
    );
    assert!(
        report.contains("CARGO_MANIFEST_DIR"),
        "wrong report: {report}"
    );
}
