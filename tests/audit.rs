//! End-to-end evidence for the fixed-priority topology audit.
//!
//! Covers exactly the scenarios the task calls for: flipped faces,
//! boundary edges, non-manifold edges, two shells sharing a single vertex,
//! disjoint closed shells, legal closed bodies (sphere and torus), and the
//! hard parse rejections (unknown vertex, duplicate undirected face, extra
//! fields and friends).

use meshcheck::{analyze, analyze_boundary, EdgeFault, Report};

fn analyze_fixture(name: &str) -> Report {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/");
    analyze(&std::fs::read_to_string(format!("{path}{name}")).unwrap())
}

fn analyze_boundary_fixture(name: &str) -> Report {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/");
    analyze_boundary(&std::fs::read_to_string(format!("{path}{name}")).unwrap())
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

// ---------- legal boundary surfaces ---------------------------------------

#[test]
fn disk_reports_one_induced_boundary_loop_and_genus_zero() {
    assert_eq!(
        analyze_boundary_fixture("boundary.mesh"),
        Report::BoundaryOk {
            vertices: 5,
            edges: 9,
            faces: 5,
            euler: 1,
            genus: 0,
            boundary_loops: vec![vec!["a".into(), "b".into(), "t".into()]],
        }
    );
}

#[test]
fn annulus_reports_two_canonical_loops_covering_every_boundary_edge() {
    let report = analyze_boundary_fixture("annulus.mesh");
    let Report::BoundaryOk {
        vertices,
        edges,
        faces,
        euler,
        genus,
        boundary_loops,
    } = report
    else {
        panic!("expected boundary-ok, got {report:?}");
    };

    assert_eq!((vertices, edges, faces, euler, genus), (6, 9, 6, 3, 0));
    assert_eq!(
        boundary_loops,
        vec![
            vec!["a".into(), "b".into(), "c".into()],
            vec!["d".into(), "f".into(), "e".into()],
        ]
    );

    let covered: std::collections::BTreeSet<[String; 2]> = boundary_loops
        .iter()
        .flat_map(|loop_vertices| {
            let mut edges = Vec::new();
            for i in 0..loop_vertices.len() {
                edges.push([
                    loop_vertices[i].clone(),
                    loop_vertices[(i + 1) % loop_vertices.len()].clone(),
                ]);
            }
            edges
        })
        .collect();
    assert_eq!(
        covered,
        [
            ["a", "b"],
            ["b", "c"],
            ["c", "a"],
            ["d", "f"],
            ["f", "e"],
            ["e", "d"],
        ]
        .into_iter()
        .map(|edge| [edge[0].into(), edge[1].into()])
        .collect()
    );
}

#[test]
fn closed_body_is_also_valid_in_boundary_mode_with_zero_loops() {
    assert_eq!(
        analyze_boundary_fixture("tet.mesh"),
        Report::BoundaryOk {
            vertices: 4,
            edges: 6,
            faces: 4,
            euler: 2,
            genus: 0,
            boundary_loops: vec![],
        }
    );
}

#[test]
fn torn_aperture_has_two_path_links_at_shared_vertex() {
    assert_eq!(
        analyze_boundary_fixture("torn-aperture.mesh"),
        Report::VertexFailed {
            id: "a".into(),
            sectors: 2,
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
fn flipped_face_still_fails_at_edge_stage_in_boundary_mode() {
    assert_eq!(
        analyze_boundary_fixture("flipped.mesh"),
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

#[test]
fn edge_shared_by_three_faces_remains_nonmanifold_with_boundaries() {
    assert!(matches!(
        analyze_boundary_fixture("nonmanifold-edge.mesh"),
        Report::EdgeFailed(f) if f.a == "a" && f.b == "b" && f.fault == EdgeFault::Nonmanifold
    ));
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
fn shared_vertex_double_shell_remains_failed_in_boundary_mode() {
    assert_eq!(
        analyze_boundary_fixture("two-shells-shared-vertex.mesh"),
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

#[test]
fn isolated_vertex_and_disconnectivity_remain_failures_in_boundary_mode() {
    let isolated = "\
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
        analyze_boundary(isolated),
        Report::VertexFailed {
            id: "lonely".into(),
            sectors: 0,
        }
    );
    assert!(matches!(
        analyze_boundary_fixture("disjoint-shells.mesh"),
        Report::ComponentFailed { id } if id == "e"
    ));
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
