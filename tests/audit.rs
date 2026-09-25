//! End-to-end evidence for the fixed-priority topology audit.
//!
//! Covers exactly the scenarios the task calls for: flipped faces,
//! boundary edges, non-manifold edges, two shells sharing a single vertex,
//! disjoint closed shells, legal closed bodies (sphere and torus), and the
//! hard parse rejections (unknown vertex, duplicate undirected face, extra
//! fields and friends).

use meshcheck::{analyze, analyze_with_boundary, EdgeFault, Report};

fn analyze_fixture(name: &str) -> Report {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/");
    analyze(&std::fs::read_to_string(format!("{path}{name}")).unwrap())
}

// ---------- legal closed bodies ------------------------------------------

#[test]
fn tetrahedron_is_a_closed_sphere() {
    assert_eq!(
        analyze_fixture("tet.mesh"),
        Report::Ok {
            vertices: 4,
            edges: 6,
            faces: 4,
            euler: 2,
            genus: 0,
        }
    );
}

#[test]
fn torus9_has_euler_characteristic_zero_and_genus_one() {
    assert_eq!(
        analyze_fixture("torus9.mesh"),
        Report::Ok {
            vertices: 9,
            edges: 27,
            faces: 18,
            euler: 0,
            genus: 1,
        }
    );
}

#[test]
fn connected_sum_of_two_tori_has_genus_two() {
    // Two tori each with one face removed, glued along the boundary with
    // reversed orientation: V=15 E=51 F=34, chi = -2, g = 2.
    assert_eq!(
        analyze_fixture("genus2.mesh"),
        Report::Ok {
            vertices: 15,
            edges: 51,
            faces: 34,
            euler: -2,
            genus: 2,
        }
    );
}

// ---------- edge stage -----------------------------------------------------

#[test]
fn flipped_face_witnesses_smallest_same_direction_edge() {
    // a-b, a-c and b-c are all traversed the same way twice; a-b is
    // lexicographically smallest.
    assert_eq!(
        analyze_fixture("flipped.mesh"),
        Report::EdgeFailed(meshcheck::EdgeFailure {
            a: "a".into(),
            b: "b".into(),
            uses: 2,
            fault: EdgeFault::Misoriented,
        })
    );
}

#[test]
fn boundary_edge_is_reported_before_vertex_or_component_matters() {
    match analyze_fixture("boundary.mesh") {
        Report::EdgeFailed(f) => {
            assert_eq!(f.a, "a");
            assert_eq!(f.b, "b");
            assert_eq!(f.uses, 1);
            assert_eq!(f.fault, EdgeFault::Boundary);
        }
        other => panic!("expected boundary edge failure, got {other:?}"),
    }
}

#[test]
fn edge_shared_by_three_faces_is_nonmanifold() {
    match analyze_fixture("nonmanifold-edge.mesh") {
        Report::EdgeFailed(f) => {
            assert_eq!((f.a.as_str(), f.b.as_str()), ("a", "b"));
            assert_eq!(f.uses, 6);
            assert_eq!(f.fault, EdgeFault::Nonmanifold);
        }
        other => panic!("expected non-manifold edge failure, got {other:?}"),
    }
}

// ---------- vertex-sector stage -------------------------------------------

#[test]
fn two_shells_sharing_one_vertex_pin_two_sectors_at_that_vertex() {
    // Both shells are separately closed; edges all pair oppositely, but the
    // link of the shared vertex a is two disjoint cycles.
    assert_eq!(
        analyze_fixture("two-shells-shared-vertex.mesh"),
        Report::VertexFailed {
            id: "a".into(),
            sectors: 2,
        }
    );
}

#[test]
fn isolated_declared_vertex_is_a_zero_sector_failure() {
    let doc = "\
v a
v b
v c
v d
v lonely
f a b c
f a c d
f a d b
f b d c
";
    assert_eq!(
        analyze(doc),
        Report::VertexFailed {
            id: "lonely".into(),
            sectors: 0,
        }
    );
}

// ---------- global-connectivity stage -------------------------------------

#[test]
fn disjoint_closed_shells_fail_global_connectivity() {
    assert_eq!(
        analyze_fixture("disjoint-shells.mesh"),
        Report::ComponentFailed { id: "e".into() }
    );
}

// ---------- parse rejection evidence --------------------------------------

#[test]
fn unknown_vertex_is_rejected_with_line_and_id() {
    match analyze_fixture("unknown-vertex.mesh") {
        Report::Rejected(meshcheck::Reject::UnknownVertex { line, id }) => {
            assert_eq!(line, 9);
            assert_eq!(id, "x");
        }
        other => panic!("expected unknown vertex rejection, got {other:?}"),
    }
}

#[test]
fn duplicate_undirected_face_is_rejected_even_when_reversed() {
    match analyze_fixture("duplicate-face.mesh") {
        Report::Rejected(meshcheck::Reject::DuplicateFace { line }) => {
            assert_eq!(line, 11);
        }
        other => panic!("expected duplicate face rejection, got {other:?}"),
    }
}

#[test]
fn extra_field_is_rejected_as_bad_face_arity() {
    match analyze_fixture("extra-field.mesh") {
        Report::Rejected(meshcheck::Reject::BadFaceArity { line }) => {
            assert_eq!(line, 8);
        }
        other => panic!("expected bad face arity rejection, got {other:?}"),
    }
}

#[test]
fn face_with_repeated_vertex_is_rejected() {
    let doc = "v a\nv b\nv c\nv d\nf a b a\nf a c d\nf a d b\nf b d c\n";
    match analyze(doc) {
        Report::Rejected(meshcheck::Reject::RepeatedVertex { line }) => assert_eq!(line, 5),
        other => panic!("expected repeated vertex rejection, got {other:?}"),
    }
}

#[test]
fn malformed_id_and_unknown_line_are_rejected() {
    let doc = "v a!\nv b\nv c\nv d\nf a b c\nf a c d\nf a d b\nf b d c\n";
    match analyze(doc) {
        Report::Rejected(meshcheck::Reject::BadId { line, id }) => {
            assert_eq!(line, 1);
            assert_eq!(id, "a!");
        }
        other => panic!("expected bad id rejection, got {other:?}"),
    }

    let doc = "v a\nv b\nv c\nv d\nxyz a b\nf a b c\nf a c d\nf a d b\nf b d c\n";
    assert!(matches!(
        analyze(doc),
        Report::Rejected(meshcheck::Reject::UnknownLine { line: 5 })
    ));
}

#[test]
fn duplicate_vertex_declaration_is_rejected() {
    let doc = "v a\nv b\nv c\nv d\nv a\nf a b c\nf a c d\nf a d b\nf b d c\n";
    assert!(matches!(
        analyze(doc),
        Report::Rejected(meshcheck::Reject::DuplicateVertex { line: 5, .. })
    ));
}

#[test]
fn short_and_long_documents_are_rejected_by_count() {
    let three_vertices = "v a\nv b\nv c\n";
    assert_eq!(
        analyze(three_vertices),
        Report::Rejected(meshcheck::Reject::BadVertexCount { count: 3 })
    );

    let no_faces = "v a\nv b\nv c\nv d\n";
    assert!(matches!(
        analyze(no_faces),
        Report::Rejected(meshcheck::Reject::BadFaceCount { count: 0 })
    ));

    // Vertex count is checked before face count.
    let both_bad = "v a\nv b\nv c\n";
    assert!(matches!(
        analyze(both_bad),
        Report::Rejected(meshcheck::Reject::BadVertexCount { count: 3 })
    ));
}

#[test]
fn limits_are_inclusive_and_beyond_them_is_rejected() {
    // 501 declared vertices exceeds the 500 cap.
    let mut too_many_vertices = String::new();
    for i in 0..501 {
        too_many_vertices.push_str(&format!("v n{i}\n"));
    }
    too_many_vertices.push_str("f n0 n1 n2\nf n0 n2 n3\nf n0 n3 n1\nf n1 n3 n2\n");
    assert!(matches!(
        analyze(&too_many_vertices),
        Report::Rejected(meshcheck::Reject::BadVertexCount { count: 501 })
    ));

    // 2001 distinct undirected triangles exceeds the 2000 face cap. They
    // need not form a surface: the count check happens before auditing.
    let mut too_many_faces = String::from("v 0\nv 1\n");
    for i in 2..500 {
        too_many_faces.push_str(&format!("v {i}\n"));
    }
    let mut made = 0;
    'outer: for i in 0..500usize {
        for j in (i + 1)..500 {
            for k in (j + 1)..500 {
                too_many_faces.push_str(&format!("f {i} {j} {k}\n"));
                made += 1;
                if made == 2001 {
                    break 'outer;
                }
            }
        }
    }
    assert_eq!(made, 2001);
    assert!(matches!(
        analyze(&too_many_faces),
        Report::Rejected(meshcheck::Reject::BadFaceCount { count: 2001 })
    ));
}

#[test]
fn comments_and_blank_lines_are_ignored() {
    let doc = "\
# leading comment
v a
  # indented comment between records
v b
v c
v d
f a b c   # inline comment
f a c d
f a d b
f b d c
";
    assert_eq!(
        analyze(doc),
        Report::Ok {
            vertices: 4,
            edges: 6,
            faces: 4,
            euler: 2,
            genus: 0,
        }
    );
}

// ---------- fixed priority semantics --------------------------------------

#[test]
fn vertex_failure_takes_priority_over_disconnectivity() {
    // Shared-vertex shells are disconnected too; the vertex-sector stage
    // must win because it runs first.
    assert!(matches!(
        analyze_fixture("two-shells-shared-vertex.mesh"),
        Report::VertexFailed { .. }
    ));
}

#[test]
fn minimal_id_witness_uses_lexicographic_vertex_order() {
    // Disjoint shells: smallest vertex outside the component of 'a' is 'e'.
    let r = analyze_fixture("disjoint-shells.mesh");
    assert_eq!(r, Report::ComponentFailed { id: "e".into() });
}

// ===========================================================================
// boundary audit mode
// ===========================================================================

/// Assert that the loops partition: disjoint vertex sets, each one a closed
/// traversal of distinct vertices, canonically rotated (smallest id first)
/// and the list sorted by leading vertex. Returns the flattened vertex set
/// so callers can assert exactly the rim vertices are covered.
fn assert_loop_coverage(loops: &[Vec<String>]) -> std::collections::BTreeSet<String> {
    let mut seen: std::collections::BTreeSet<String> = Default::default();
    let mut firsts: Vec<String> = Vec::new();
    for loop_ in loops {
        assert!(loop_.len() >= 3, "a boundary loop has at least 3 edges");
        // Rotated to start at the smallest vertex id.
        let min = loop_.iter().min().unwrap();
        assert_eq!(&loop_[0], min, "loop must start at its smallest vertex");
        // Distinct vertices within the loop (a simple cycle).
        let unique: std::collections::BTreeSet<&String> = loop_.iter().collect();
        assert_eq!(unique.len(), loop_.len(), "loop revisits a vertex");
        // Loops are mutually vertex-disjoint.
        for v in loop_ {
            assert!(seen.insert(v.clone()), "vertex {v} in two loops");
        }
        firsts.push(loop_[0].clone());
    }
    let mut sorted_firsts = firsts.clone();
    sorted_firsts.sort();
    assert_eq!(
        firsts, sorted_firsts,
        "loops must be sorted by first vertex"
    );
    seen
}

// ---------- legal surfaces with boundary ----------------------------------

#[test]
fn disc_has_one_canonical_boundary_loop_and_genus_zero() {
    match analyze_with_boundary(
        &std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/examples/disc.mesh"))
            .unwrap(),
    ) {
        Report::BoundaryOk {
            vertices,
            edges,
            faces,
            euler,
            genus,
            boundary,
        } => {
            assert_eq!((vertices, edges, faces), (5, 8, 4));
            assert_eq!(euler, 1);
            assert_eq!(genus, 0);
            assert_eq!(boundary, vec![vec!["a", "b", "c", "d"]]);
            let covered = assert_loop_coverage(&boundary);
            assert_eq!(
                covered,
                ["a", "b", "c", "d"].into_iter().map(String::from).collect()
            );
        }
        other => panic!("expected BoundaryOk, got {other:?}"),
    }
}

#[test]
fn annulus_has_two_nonoverlapping_loops_sorted_by_leading_vertex() {
    match analyze_with_boundary(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/examples/annulus.mesh"
        ))
        .unwrap(),
    ) {
        Report::BoundaryOk {
            vertices,
            edges,
            faces,
            euler,
            genus,
            boundary,
        } => {
            assert_eq!((vertices, edges, faces), (8, 16, 8));
            assert_eq!(euler, 0); // chi = 2 - 2*0 - 2
            assert_eq!(genus, 0);
            assert_eq!(boundary.len(), 2);
            assert_eq!(boundary[0], vec!["a", "b", "c", "d"]);
            assert_eq!(boundary[1], vec!["e", "h", "g", "f"]);
            let covered = assert_loop_coverage(&boundary);
            assert_eq!(
                covered,
                ["a", "b", "c", "d", "e", "f", "g", "h"]
                    .into_iter()
                    .map(String::from)
                    .collect()
            );
        }
        other => panic!("expected BoundaryOk, got {other:?}"),
    }
}

#[test]
fn punctured_torus_has_genus_one_with_one_boundary() {
    // torus9 with one face removed: chi = 9-27+17 = -1, b = 1, g = 1.
    match analyze_with_boundary(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/examples/punctured-torus.mesh"
        ))
        .unwrap(),
    ) {
        Report::BoundaryOk {
            euler,
            genus,
            boundary,
            ..
        } => {
            assert_eq!(euler, -1);
            assert_eq!(genus, 1);
            // The loop follows the direction induced by the removed face's
            // three single-use edges: 0 -> 4 -> 1 -> 0 (never reversed).
            assert_eq!(boundary, vec![vec!["0", "4", "1"]]);
            assert_loop_coverage(&boundary);
        }
        other => panic!("expected BoundaryOk, got {other:?}"),
    }
}

#[test]
fn capped_bipyramid_with_hole_audits_as_a_disc() {
    // The existing boundary.mesh fixture is a closed-mode failure but a
    // perfectly good disc under boundary audit.
    match analyze_fixture_boundary("boundary.mesh") {
        Report::BoundaryOk {
            vertices,
            edges,
            faces,
            euler,
            genus,
            boundary,
        } => {
            assert_eq!((vertices, edges, faces), (5, 9, 5));
            assert_eq!(euler, 1);
            assert_eq!(genus, 0);
            assert_eq!(boundary, vec![vec!["a", "t", "b"]]);
            assert_loop_coverage(&boundary);
        }
        other => panic!("expected BoundaryOk, got {other:?}"),
    }
}

#[test]
fn boundary_loop_starts_at_smallest_vertex_regardless_of_face_order() {
    // Same disc declared with the rim rotated; the loop must still lead
    // with the lexicographically smallest vertex a and keep its direction.
    let doc = "\
v a
v b
v c
v d
v o
f o b c
f o c d
f o d a
f o a b
";
    match analyze_with_boundary(doc) {
        Report::BoundaryOk { boundary, .. } => {
            assert_eq!(boundary, vec![vec!["a", "b", "c", "d"]]);
        }
        other => panic!("expected BoundaryOk, got {other:?}"),
    }
}

// ---------- closed bodies remain valid in boundary mode -------------------

#[test]
fn closed_bodies_have_zero_boundary_loops_in_boundary_mode() {
    for name in ["tet.mesh", "torus9.mesh", "genus2.mesh"] {
        let closed = analyze_fixture(name);
        let with_boundary = analyze_fixture_boundary(name);
        match (closed, with_boundary) {
            (
                Report::Ok {
                    vertices: v1,
                    edges: e1,
                    faces: f1,
                    euler: x1,
                    genus: g1,
                },
                Report::BoundaryOk {
                    vertices: v2,
                    edges: e2,
                    faces: f2,
                    euler: x2,
                    genus: g2,
                    boundary,
                },
            ) => {
                assert_eq!((v1, e1, f1, x1, g1), (v2, e2, f2, x2, g2));
                assert!(boundary.is_empty(), "{name} must report no holes");
            }
            other => panic!("{name}: expected matching ok reports, got {other:?}"),
        }
    }
}

// ---------- edge-stage compatibility ---------------------------------------

#[test]
fn flipped_face_fails_edges_first_in_boundary_mode() {
    assert_eq!(
        analyze_fixture_boundary("flipped.mesh"),
        Report::EdgeFailed(meshcheck::EdgeFailure {
            a: "a".into(),
            b: "b".into(),
            uses: 2,
            fault: EdgeFault::Misoriented,
        })
    );
}

#[test]
fn same_direction_pair_outranks_coexisting_boundary_edges() {
    // A disc with a fin glued along a-b in the same direction: a-b and
    // a-x's rim are both affected; edge priority still reports the
    // misoriented pair, not a boundary.
    let doc = "\
v a
v b
v c
v d
v o
v x
v y
f o a b
f o b c
f o c d
f o d a
f a b x
f b x y
f x y a
";
    assert!(matches!(
        analyze_with_boundary(doc),
        Report::EdgeFailed(f) if f.a == "a" && f.b == "b" && f.fault == EdgeFault::Misoriented
    ));
}

#[test]
fn three_face_edge_still_nonmanifold_in_boundary_mode() {
    match analyze_fixture_boundary("nonmanifold-edge.mesh") {
        Report::EdgeFailed(f) => {
            assert_eq!((f.a.as_str(), f.b.as_str()), ("a", "b"));
            assert_eq!(f.uses, 6);
            assert_eq!(f.fault, EdgeFault::Nonmanifold);
        }
        other => panic!("expected non-manifold edge failure, got {other:?}"),
    }
}

// ---------- vertex-stage compatibility -------------------------------------

#[test]
fn shared_vertex_double_shell_still_fans_two_sectors_in_boundary_mode() {
    assert_eq!(
        analyze_fixture_boundary("two-shells-shared-vertex.mesh"),
        Report::VertexFailed {
            id: "a".into(),
            sectors: 2,
        }
    );
}

#[test]
fn isolated_vertex_is_rejected_even_in_boundary_mode() {
    let doc = "\
v a
v b
v c
v d
v o
v lonely
f o a b
f o b c
f o c d
f o d a
";
    assert_eq!(
        analyze_with_boundary(doc),
        Report::VertexFailed {
            id: "lonely".into(),
            sectors: 0,
        }
    );
}

// ---------- broken aperture ------------------------------------------------

#[test]
fn torn_aperture_flap_sharing_one_vertex_fails_vertex_stage() {
    // The disc's own boundary loop would be legal, but the loose triangular
    // flap x-y-z is attached only at rim vertex a: the link of a has two
    // sectors, so no loops may be published.
    assert_eq!(
        analyze_fixture_boundary("torn-aperture.mesh"),
        Report::VertexFailed {
            id: "a".into(),
            sectors: 2,
        }
    );
}

#[test]
fn edge_connected_shells_fall_through_to_component_stage_in_boundary_mode() {
    // Two closed tetrahedra with no single-use edges: edges and vertex
    // sectors pass, but the complex is not edge-connected.
    assert_eq!(
        analyze_fixture_boundary("disjoint-shells.mesh"),
        Report::ComponentFailed { id: "e".into() }
    );
}

// ---------- default mode is byte-for-byte the old audit --------------------

#[test]
fn default_closed_mode_is_unchanged_for_boundary_inputs() {
    // Every open fixture must remain a plain boundary edge failure under
    // the default entry point.
    for name in [
        "disc.mesh",
        "annulus.mesh",
        "boundary.mesh",
        "torn-aperture.mesh",
    ] {
        match analyze_fixture(name) {
            Report::EdgeFailed(f) => assert_eq!(f.fault, EdgeFault::Boundary, "{name}"),
            other => panic!("{name}: expected closed-mode boundary failure, got {other:?}"),
        }
    }
}

fn analyze_fixture_boundary(name: &str) -> Report {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/");
    analyze_with_boundary(&std::fs::read_to_string(format!("{path}{name}")).unwrap())
}
