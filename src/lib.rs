//! meshcheck — combinatorial topology auditor for directed triangle soups.
//!
//! The default audit proves exactly one property: that the input is a
//! *combinatorially watertight* orientable triangle mesh — every undirected
//! edge is used by exactly two faces with opposite directions, the faces
//! around every vertex form a single fan (a cyclic link), and the face
//! complex is edge-connected. [`AuditMode::Boundary`] additionally accepts
//! compact surfaces with boundary, where a vertex link may be either one
//! cycle or one simple path. It does **not** inspect coordinates and
//! therefore cannot detect geometric self-intersection or degenerate
//! embedding.

use std::collections::{BTreeMap, BTreeSet};

/// Inclusive bounds mandated by the input specification.
pub const MIN_VERTICES: usize = 4;
pub const MAX_VERTICES: usize = 500;
pub const MIN_FACES: usize = 4;
pub const MAX_FACES: usize = 2000;

/// Selects whether legal one-face edges are accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AuditMode {
    /// Every edge must have exactly two oppositely directed face uses.
    #[default]
    Closed,
    /// An edge may have one face use. Such edges form oriented boundary loops.
    Boundary,
}

/// Why the input text was rejected before any topology audit could run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reject {
    /// Line with no recognized leading keyword.
    UnknownLine { line: u32 },
    /// `v` declaration without exactly one id token.
    BadVertex { line: u32 },
    /// `f` declaration whose token count is not exactly three.
    BadFaceArity { line: u32 },
    /// Face or vertex id that is not 1–32 chars of `[A-Za-z0-9_-]`.
    BadId { line: u32, id: String },
    /// `f` whose three vertices are not pairwise distinct.
    RepeatedVertex { line: u32 },
    /// Same vertex id declared by more than one `v` line.
    DuplicateVertex { line: u32, id: String },
    /// Face references a vertex that was never declared.
    UnknownVertex { line: u32, id: String },
    /// The same (unordered) triangle appears on two different `f` lines.
    DuplicateFace { line: u32 },
    /// Fewer than 4 or more than 500 unique declared vertices.
    BadVertexCount { count: usize },
    /// Fewer than 4 or more than 2000 faces.
    BadFaceCount { count: usize },
}

/// The kind of failure found while auditing undirected edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeFault {
    /// The edge belongs to only one directed face (a mesh boundary).
    Boundary,
    /// The edge occurs twice but in the same direction (a flipped face),
    /// or its two occurrences cannot be paired oppositely.
    Misoriented,
    /// The edge is incident on three or more faces (non-manifold edge).
    Nonmanifold,
}

/// A failed undirected edge, addressed by the lexicographically ordered
/// pair of its endpoint ids.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdgeFailure {
    pub a: String,
    pub b: String,
    pub uses: usize,
    pub fault: EdgeFault,
}

/// Structured result of auditing one mesh document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Report {
    Rejected(Reject),
    EdgeFailed(EdgeFailure),
    /// The faces around this vertex do not form exactly one cyclic fan in
    /// closed mode, or one simple path/cycle in boundary mode.
    VertexFailed {
        id: String,
        sectors: usize,
    },
    /// Two or more edge-connected components exist; this is the smallest
    /// vertex id outside the component containing the smallest vertex
    /// overall.
    ComponentFailed {
        id: String,
    },
    /// A closed connected orientable surface passed the default audit.
    Ok {
        vertices: usize,
        edges: usize,
        faces: usize,
        euler: i64,
        genus: u32,
    },
    /// A connected orientable surface, possibly with boundary, passed the
    /// explicit boundary audit. Each inner vector is one boundary loop.
    BoundaryOk {
        vertices: usize,
        edges: usize,
        faces: usize,
        euler: i64,
        genus: u32,
        boundary_loops: Vec<Vec<String>>,
    },
    /// All combinatorial stages passed, but `χ = 2 - 2g - b` does not yield
    /// a non-negative integer genus. No genus or successful status is
    /// reported in that case.
    InvariantFailed {
        vertices: usize,
        edges: usize,
        faces: usize,
        euler: i64,
        boundary_loops: Vec<Vec<String>>,
    },
}

/// A parsed mesh: ordered face list plus sorted vertex id inventory.
#[derive(Debug, Clone)]
pub struct Mesh {
    /// All declared vertex ids, sorted lexicographically.
    pub vertices: Vec<String>,
    /// Directed faces; each face is three vertex ids in declared order.
    pub faces: Vec<[String; 3]>,
}

impl Reject {
    pub fn line(&self) -> Option<u32> {
        match self {
            Reject::UnknownLine { line }
            | Reject::BadVertex { line }
            | Reject::BadFaceArity { line }
            | Reject::BadId { line, .. }
            | Reject::RepeatedVertex { line }
            | Reject::DuplicateVertex { line, .. }
            | Reject::UnknownVertex { line, .. }
            | Reject::DuplicateFace { line } => Some(*line),
            Reject::BadVertexCount { .. } | Reject::BadFaceCount { .. } => None,
        }
    }
}

fn valid_id(id: &str) -> bool {
    (1..=32).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// Parse a mesh document.
///
/// Grammar (one record per line, blanks and `#` comments allowed):
/// ```text
/// v <id>
/// f <id> <id> <id>
/// ```
pub fn parse(input: &str) -> Result<Mesh, Reject> {
    let mut declared: BTreeSet<String> = BTreeSet::new();
    let mut vertices: Vec<String> = Vec::new();
    let mut raw_faces: Vec<[String; 3]> = Vec::new();
    let mut seen_faces: BTreeSet<[String; 3]> = BTreeSet::new();

    for (idx, raw_line) in input.lines().enumerate() {
        let line_no = (idx + 1) as u32;
        // Strip an inline `#` comment, then ASCII-whitespace tokenize.
        let line = raw_line.split('#').next().unwrap_or("");
        let tokens: Vec<&str> = line.split_ascii_whitespace().collect();
        if tokens.is_empty() {
            continue;
        }

        match tokens[0] {
            "v" => {
                if tokens.len() != 2 {
                    return Err(Reject::BadVertex { line: line_no });
                }
                let id = tokens[1];
                if !valid_id(id) {
                    return Err(Reject::BadId {
                        line: line_no,
                        id: id.to_string(),
                    });
                }
                if !declared.insert(id.to_string()) {
                    return Err(Reject::DuplicateVertex {
                        line: line_no,
                        id: id.to_string(),
                    });
                }
                vertices.push(id.to_string());
            }
            "f" => {
                if tokens.len() != 4 {
                    return Err(Reject::BadFaceArity { line: line_no });
                }
                let face = [
                    tokens[1].to_string(),
                    tokens[2].to_string(),
                    tokens[3].to_string(),
                ];
                for id in &face {
                    if !valid_id(id) {
                        return Err(Reject::BadId {
                            line: line_no,
                            id: id.clone(),
                        });
                    }
                }
                if face[0] == face[1] || face[1] == face[2] || face[0] == face[2] {
                    return Err(Reject::RepeatedVertex { line: line_no });
                }
                // Canonical (undirected) representation of the triangle.
                let mut sorted = face.clone();
                sorted.sort();
                if !seen_faces.insert(sorted) {
                    return Err(Reject::DuplicateFace { line: line_no });
                }
                raw_faces.push(face);
            }
            _ => {
                return Err(Reject::UnknownLine { line: line_no });
            }
        }
    }

    if !(MIN_VERTICES..=MAX_VERTICES).contains(&vertices.len()) {
        return Err(Reject::BadVertexCount {
            count: vertices.len(),
        });
    }
    if !(MIN_FACES..=MAX_FACES).contains(&raw_faces.len()) {
        return Err(Reject::BadFaceCount {
            count: raw_faces.len(),
        });
    }
    for face in &raw_faces {
        for id in face {
            if !declared.contains(id) {
                // Find the face's original line number for the witness.
                // Re-scan to attribute the reference precisely.
                let line = find_face_line(input, face).unwrap_or(0);
                return Err(Reject::UnknownVertex {
                    line,
                    id: id.clone(),
                });
            }
        }
    }

    vertices.sort();
    Ok(Mesh {
        vertices,
        faces: raw_faces,
    })
}

/// Locate the 1-based line on which a given directed face was declared.
fn find_face_line(input: &str, target: &[String; 3]) -> Option<u32> {
    for (idx, raw_line) in input.lines().enumerate() {
        let line = raw_line.split('#').next().unwrap_or("");
        let tokens: Vec<&str> = line.split_ascii_whitespace().collect();
        if tokens.len() == 4
            && tokens[0] == "f"
            && tokens[1] == target[0]
            && tokens[2] == target[1]
            && tokens[3] == target[2]
        {
            return Some((idx + 1) as u32);
        }
    }
    None
}

fn undirected(u: &str, v: &str) -> (String, String) {
    if u <= v {
        (u.to_string(), v.to_string())
    } else {
        (v.to_string(), u.to_string())
    }
}

fn directed_edge_counts(mesh: &Mesh) -> BTreeMap<(String, String), [usize; 2]> {
    // undirected edge -> counts of traversal in each direction
    let mut seen: BTreeMap<(String, String), [usize; 2]> = BTreeMap::new();
    for face in &mesh.faces {
        let pairs = [
            (&face[0], &face[1]),
            (&face[1], &face[2]),
            (&face[2], &face[0]),
        ];
        for (u, v) in pairs {
            let key = undirected(u, v);
            let dir = if u == &key.0 { 0 } else { 1 };
            let entry = seen.entry(key).or_insert([0, 0]);
            entry[dir] += 1;
        }
    }
    seen
}

/// Stage 1 in the default closed mode: every undirected edge must be used by
/// exactly two directed faces, and the two uses must traverse it in opposite
/// directions.
///
/// The witness is the lexicographically smallest faulty undirected edge
/// (ordered by smaller endpoint id, then larger).
pub fn audit_edges(mesh: &Mesh) -> Result<(), EdgeFailure> {
    audit_edges_with(mesh, AuditMode::Closed)
}

/// Edge audit with an explicit boundary policy.
pub fn audit_edges_with(mesh: &Mesh, mode: AuditMode) -> Result<(), EdgeFailure> {
    let allow_boundary = mode == AuditMode::Boundary;
    for ((a, b), dirs) in directed_edge_counts(mesh) {
        let uses = dirs[0] + dirs[1];
        let fault = if uses == 1 {
            if allow_boundary {
                continue;
            }
            EdgeFault::Boundary
        } else if uses == 2 && dirs[0] == 1 && dirs[1] == 1 {
            continue; // exactly one use in each direction
        } else if uses == 2 {
            EdgeFault::Misoriented
        } else {
            EdgeFault::Nonmanifold
        };
        return Err(EdgeFailure { a, b, uses, fault });
    }
    Ok(())
}

/// Stage 2: in closed mode the faces incident on every vertex must form one
/// cyclic fan. In boundary mode that fan may instead be one simple path.
///
/// For a directed face `(x, y, z)` the link edge at vertex `v` points from
/// `v`'s successor to `v`'s predecessor. Opposite face-edge uses glue the
/// matching link endpoints. Boundary vertices therefore have a path link;
/// interior vertices have a cycle link.
pub fn audit_vertex_sectors(mesh: &Mesh) -> Result<(), (String, usize)> {
    audit_vertex_sectors_with(mesh, AuditMode::Closed)
}

/// Vertex-link audit with an explicit boundary policy.
pub fn audit_vertex_sectors_with(
    mesh: &Mesh,
    mode: AuditMode,
) -> Result<(), (String, usize)> {
    // vertex -> link source -> set of link targets, and reverse adjacency.
    // Sets make a forked link explicit even if a malformed precondition ever
    // lets duplicate directed edges reach this stage.
    let mut outgoing: BTreeMap<String, BTreeMap<String, BTreeSet<String>>> = BTreeMap::new();
    let mut incoming: BTreeMap<String, BTreeMap<String, BTreeSet<String>>> = BTreeMap::new();
    for v in &mesh.vertices {
        outgoing.insert(v.clone(), BTreeMap::new());
        incoming.insert(v.clone(), BTreeMap::new());
    }
    for face in &mesh.faces {
        let corners = [(0usize, 1usize, 2usize), (1, 2, 0), (2, 0, 1)];
        for (vi, si, pi) in corners {
            let v = &face[vi];
            let successor = face[si].clone();
            let predecessor = face[pi].clone();
            outgoing
                .get_mut(v)
                .expect("vertex already registered")
                .entry(successor.clone())
                .or_default()
                .insert(predecessor.clone());
            incoming
                .get_mut(v)
                .expect("vertex already registered")
                .entry(predecessor)
                .or_default()
                .insert(successor);
        }
    }

    for v in &mesh.vertices {
        let out = &outgoing[v];
        let inc = &incoming[v];
        let mut nodes: BTreeSet<String> = BTreeSet::new();
        for (s, targets) in out {
            nodes.insert(s.clone());
            nodes.extend(targets.iter().cloned());
        }

        if nodes.is_empty() {
            return Err((v.clone(), 0));
        }

        // A surface link has maximum degree two. A fork is a vertex failure
        // even though its weak-component count can be one.
        let forked = nodes.iter().any(|node| {
            out.get(node).map(BTreeSet::len).unwrap_or(0) > 1
                || inc.get(node).map(BTreeSet::len).unwrap_or(0) > 1
        });

        // Count weak components, treating a directed link edge as undirected
        // for sector/fan decomposition.
        let mut visited: BTreeSet<String> = BTreeSet::new();
        let mut sectors = 0usize;
        for start in &nodes {
            if visited.contains(start) {
                continue;
            }
            sectors += 1;
            let mut stack = vec![start.clone()];
            while let Some(cur) = stack.pop() {
                if !visited.insert(cur.clone()) {
                    continue;
                }
                if let Some(targets) = out.get(&cur) {
                    stack.extend(targets.iter().cloned());
                }
                if let Some(sources) = inc.get(&cur) {
                    stack.extend(sources.iter().cloned());
                }
            }
        }

        if sectors != 1 {
            return Err((v.clone(), sectors));
        }
        if forked {
            return Err((v.clone(), 1));
        }

        let link_edges: usize = out.values().map(BTreeSet::len).sum();
        let cyclic = nodes.iter().all(|node| {
            out.get(node).map(BTreeSet::len).unwrap_or(0) == 1
                && inc.get(node).map(BTreeSet::len).unwrap_or(0) == 1
        }) && link_edges == nodes.len();

        if mode == AuditMode::Closed {
            if !cyclic {
                return Err((v.clone(), 1));
            }
            continue;
        }

        let path_start = nodes
            .iter()
            .filter(|node| {
                out.get(node).map(BTreeSet::len).unwrap_or(0) == 1
                    && inc.get(node).map(BTreeSet::len).unwrap_or(0) == 0
            })
            .count();
        let path_end = nodes
            .iter()
            .filter(|node| {
                out.get(node).map(BTreeSet::len).unwrap_or(0) == 0
                    && inc.get(node).map(BTreeSet::len).unwrap_or(0) == 1
            })
            .count();
        let simple_path = link_edges + 1 == nodes.len() && path_start == 1 && path_end == 1;
        if !cyclic && !simple_path {
            return Err((v.clone(), 1));
        }
    }
    Ok(())
}

/// Stage 3: all faces must belong to one edge-connected component. Witness:
/// smallest vertex id outside the component holding the smallest vertex
/// overall.
pub fn audit_connected(mesh: &Mesh) -> Result<(), String> {
    let index: BTreeMap<&str, usize> = mesh
        .vertices
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();

    // Connect face ids when they share any undirected edge.
    let mut edge_faces: BTreeMap<(String, String), Vec<usize>> = BTreeMap::new();
    for (fi, face) in mesh.faces.iter().enumerate() {
        let edges = [
            undirected(&face[0], &face[1]),
            undirected(&face[1], &face[2]),
            undirected(&face[2], &face[0]),
        ];
        for edge in edges {
            edge_faces.entry(edge).or_default().push(fi);
        }
    }

    // The smallest declared vertex is the root witness. A mesh reaching this
    // stage uses every declared vertex, so it is incident on some face.
    let root_vertex = mesh.vertices[0].as_str();
    let start_face = mesh
        .faces
        .iter()
        .position(|face| face.iter().any(|id| id.as_str() == root_vertex))
        .expect("root vertex is used after vertex audit");

    let mut visited_faces = vec![false; mesh.faces.len()];
    let mut stack = vec![start_face];
    visited_faces[start_face] = true;
    while let Some(fi) = stack.pop() {
        let face = &mesh.faces[fi];
        let edges = [
            undirected(&face[0], &face[1]),
            undirected(&face[1], &face[2]),
            undirected(&face[2], &face[0]),
        ];
        for edge in edges {
            for &next in &edge_faces[&edge] {
                if !visited_faces[next] {
                    visited_faces[next] = true;
                    stack.push(next);
                }
            }
        }
    }

    let mut reached = vec![false; mesh.vertices.len()];
    for (fi, face) in mesh.faces.iter().enumerate() {
        if visited_faces[fi] {
            for id in face {
                reached[index[id.as_str()]] = true;
            }
        }
    }

    for (i, id) in mesh.vertices.iter().enumerate() {
        if !reached[i] {
            return Err(id.clone());
        }
    }
    Ok(())
}

/// Number of distinct undirected edges in the parsed mesh.
pub fn edge_count(mesh: &Mesh) -> usize {
    directed_edge_counts(mesh).len()
}

/// Return the one-face directed edges, in the orientation induced by their
/// sole face.
fn directed_boundary_edges(mesh: &Mesh) -> Vec<(String, String)> {
    let mut boundaries = Vec::new();
    for ((a, b), dirs) in directed_edge_counts(mesh) {
        if dirs[0] + dirs[1] == 1 {
            if dirs[0] == 1 {
                boundaries.push((a, b));
            } else {
                boundaries.push((b, a));
            }
        }
    }
    boundaries
}

/// Extract pairwise vertex-disjoint oriented boundary loops.
///
/// Each loop is rotated so its smallest id is first, without reversing its
/// induced direction. Loops are sorted lexicographically by their first id.
fn boundary_loops(mesh: &Mesh) -> Result<Vec<Vec<String>>, String> {
    let mut outgoing: BTreeMap<String, String> = BTreeMap::new();
    let mut indegree: BTreeMap<String, usize> = BTreeMap::new();
    for (u, v) in directed_boundary_edges(mesh) {
        indegree.entry(u.clone()).or_insert(0);
        if outgoing.insert(u.clone(), v.clone()).is_some() {
            return Err(u);
        }
        *indegree.entry(v).or_insert(0) += 1;
    }

    let mut unvisited: BTreeSet<String> = outgoing.keys().cloned().collect();
    let mut loops = Vec::new();
    while let Some(start) = unvisited.iter().next().cloned() {
        let mut vertices = Vec::new();
        let mut current = start.clone();
        loop {
            if !unvisited.remove(&current) {
                return Err(current);
            }
            vertices.push(current.clone());
            let Some(next) = outgoing.get(&current) else {
                return Err(current);
            };
            if next == &start {
                break;
            }
            if !unvisited.contains(next) || indegree.get(next).copied().unwrap_or(0) != 1 {
                return Err(next.clone());
            }
            current = next.clone();
        }

        // Explicit canonical rotation. Traversal order is never reversed.
        let min_pos = vertices
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| a.cmp(b))
            .map(|(i, _)| i)
            .expect("boundary loop is nonempty");
        vertices.rotate_left(min_pos);
        loops.push(vertices);
    }

    loops.sort_by(|a, b| a[0].cmp(&b[0]));
    Ok(loops)
}

fn euler_report(
    mesh: &Mesh,
    mode: AuditMode,
    boundary_loops: Vec<Vec<String>>,
) -> Report {
    let v = mesh.vertices.len();
    let f = mesh.faces.len();
    let e = edge_count(mesh);
    let euler = v as i64 - e as i64 + f as i64;
    let boundaries = boundary_loops.len() as i64;

    // For a connected orientable compact surface: chi = 2 - 2g - b.
    // Reject the invariant rather than clamping or rounding if it cannot be
    // solved for a non-negative integer genus.
    let twice_genus = 2 - euler - boundaries;
    if twice_genus < 0 || twice_genus % 2 != 0 {
        return Report::InvariantFailed {
            vertices: v,
            edges: e,
            faces: f,
            euler,
            boundary_loops,
        };
    }
    let genus = (twice_genus / 2) as u32;

    match mode {
        AuditMode::Closed => Report::Ok {
            vertices: v,
            edges: e,
            faces: f,
            euler,
            genus,
        },
        AuditMode::Boundary => Report::BoundaryOk {
            vertices: v,
            edges: e,
            faces: f,
            euler,
            genus,
            boundary_loops,
        },
    }
}

/// Run the fixed-priority closed audit:
/// parse → edges → vertex links → edge connectivity → invariants.
pub fn analyze(input: &str) -> Report {
    analyze_with(input, AuditMode::Closed)
}

/// Run the fixed-priority closed audit, but accept legal boundary edges.
pub fn analyze_boundary(input: &str) -> Report {
    analyze_with(input, AuditMode::Boundary)
}

/// Run the full audit under an explicit boundary policy.
pub fn analyze_with(input: &str, mode: AuditMode) -> Report {
    let mesh = match parse(input) {
        Ok(mesh) => mesh,
        Err(reject) => return Report::Rejected(reject),
    };

    if let Err(failure) = audit_edges_with(&mesh, mode) {
        return Report::EdgeFailed(failure);
    }
    if let Err((id, sectors)) = audit_vertex_sectors_with(&mesh, mode) {
        return Report::VertexFailed { id, sectors };
    }
    if let Err(id) = audit_connected(&mesh) {
        return Report::ComponentFailed { id };
    }

    let loops = match boundary_loops(&mesh) {
        Ok(loops) => loops,
        Err(id) => return Report::VertexFailed { id, sectors: 1 },
    };
    euler_report(&mesh, mode, loops)
}
