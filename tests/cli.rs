//! CLI contract tests: rendered output lines and exit codes.

use std::io::Write;
use std::process::{Command, Stdio};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_meshcheck")
}

fn run_file(name: &str) -> (String, i32) {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/");
    run_args(&[&format!("{path}{name}")])
}

fn run_args(args: &[&str]) -> (String, i32) {
    let out = Command::new(bin())
        .args(args)
        .output()
        .expect("meshcheck binary runs");
    (
        String::from_utf8(out.stdout).unwrap().trim().to_string(),
        out.status.code().unwrap(),
    )
}

fn run_file_boundary(name: &str) -> (String, i32) {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/");
    run_args(&["--boundary", &format!("{path}{name}")])
}

#[test]
fn valid_tet_prints_invariants_and_exits_zero() {
    assert_eq!(
        run_file("tet.mesh"),
        ("ok V=4 E=6 F=4 chi=2 genus=0".to_string(), 0)
    );
    assert_eq!(
        run_file("torus9.mesh"),
        ("ok V=9 E=27 F=18 chi=0 genus=1".to_string(), 0)
    );
}

#[test]
fn boundary_exits_two_with_edge_failure_line() {
    assert_eq!(
        run_file("boundary.mesh"),
        ("edge-fail edge=a-b uses=1 fault=boundary".to_string(), 2)
    );
}

#[test]
fn flipped_exits_two_with_misoriented_witness() {
    assert_eq!(
        run_file("flipped.mesh"),
        ("edge-fail edge=a-b uses=2 fault=misoriented".to_string(), 2)
    );
}

#[test]
fn pinched_shells_exits_three() {
    assert_eq!(
        run_file("two-shells-shared-vertex.mesh"),
        ("vertex-fail vertex=a sectors=2".to_string(), 3)
    );
}

#[test]
fn disjoint_shells_exits_four() {
    assert_eq!(
        run_file("disjoint-shells.mesh"),
        ("component-fail vertex=e".to_string(), 4)
    );
}

#[test]
fn rejected_documents_exit_one() {
    assert_eq!(
        run_file("unknown-vertex.mesh"),
        ("reject kind=unknown_vertex line=9 id=x".to_string(), 1)
    );
    assert_eq!(
        run_file("duplicate-face.mesh"),
        ("reject kind=duplicate_face line=11".to_string(), 1)
    );
    assert_eq!(
        run_file("extra-field.mesh"),
        ("reject kind=bad_face_arity line=8".to_string(), 1)
    );
}

#[test]
fn reads_directed_faces_from_stdin() {
    let doc = "\
v a
v b
v c
v d
f a b c
f a c d
f a d b
f b d c
";
    let mut child = Command::new(bin())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(doc.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code().unwrap(), 0);
    assert_eq!(
        String::from_utf8(out.stdout).unwrap().trim(),
        "ok V=4 E=6 F=4 chi=2 genus=0"
    );
}

#[test]
fn help_request_exits_three_and_mentions_usage() {
    let out = Command::new(bin()).arg("--help").output().unwrap();
    assert_eq!(out.status.code().unwrap(), 3);
    assert!(String::from_utf8(out.stdout)
        .unwrap()
        .contains("topology auditor"));
}

#[test]
fn missing_file_is_a_usage_error() {
    let out = Command::new(bin())
        .arg("/nonexistent/mesh/does-not-exist.mesh")
        .output()
        .unwrap();
    assert_eq!(out.status.code().unwrap(), 3);
    assert!(String::from_utf8(out.stderr)
        .unwrap()
        .contains("failed to read"));
}

// ---------- boundary mode --------------------------------------------------

#[test]
fn boundary_mode_disc_prints_summary_and_loop_exits_zero() {
    assert_eq!(
        run_file_boundary("disc.mesh"),
        (
            "ok-boundary V=5 E=8 F=4 chi=1 genus=0 holes=1\nboundary a-b-c-d".to_string(),
            0
        )
    );
    // The short flag means the same thing.
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/disc.mesh");
    assert_eq!(
        run_args(&["-b", path]),
        (
            "ok-boundary V=5 E=8 F=4 chi=1 genus=0 holes=1\nboundary a-b-c-d".to_string(),
            0
        )
    );
}

#[test]
fn boundary_mode_annulus_lists_two_loops_sorted_by_first_vertex() {
    assert_eq!(
        run_file_boundary("annulus.mesh"),
        (
            "ok-boundary V=8 E=16 F=8 chi=0 genus=0 holes=2\nboundary a-b-c-d\nboundary e-h-g-f"
                .to_string(),
            0
        )
    );
}

#[test]
fn boundary_mode_punctured_torus_reports_genus_one() {
    assert_eq!(
        run_file_boundary("punctured-torus.mesh"),
        (
            "ok-boundary V=9 E=27 F=17 chi=-1 genus=1 holes=1\nboundary 0-4-1".to_string(),
            0
        )
    );
}

#[test]
fn boundary_mode_closed_body_reports_zero_holes() {
    assert_eq!(
        run_file_boundary("tet.mesh"),
        (
            "ok-boundary V=4 E=6 F=4 chi=2 genus=0 holes=0".to_string(),
            0
        )
    );
}

#[test]
fn boundary_mode_keeps_edge_failure_priority_and_exit_two() {
    assert_eq!(
        run_file_boundary("flipped.mesh"),
        ("edge-fail edge=a-b uses=2 fault=misoriented".to_string(), 2)
    );
}

#[test]
fn boundary_mode_torn_aperture_exits_three() {
    assert_eq!(
        run_file_boundary("torn-aperture.mesh"),
        ("vertex-fail vertex=a sectors=2".to_string(), 3)
    );
}

#[test]
fn boundary_mode_disconnected_exits_four() {
    assert_eq!(
        run_file_boundary("disjoint-shells.mesh"),
        ("component-fail vertex=e".to_string(), 4)
    );
}

#[test]
fn boundary_mode_reads_stdin_and_prints_loop() {
    let doc = "\
v a
v b
v c
v d
v o
f o a b
f o b c
f o c d
f o d a
";
    let mut child = Command::new(bin())
        .arg("--boundary")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(doc.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code().unwrap(), 0);
    assert_eq!(
        String::from_utf8(out.stdout).unwrap().trim(),
        "ok-boundary V=5 E=8 F=4 chi=1 genus=0 holes=1\nboundary a-b-c-d"
    );
}

#[test]
fn unknown_option_is_a_usage_error() {
    let out = Command::new(bin())
        .arg("--watertight-only")
        .output()
        .unwrap();
    assert_eq!(out.status.code().unwrap(), 3);
    assert!(String::from_utf8(out.stderr)
        .unwrap()
        .contains("unknown option"));
}
