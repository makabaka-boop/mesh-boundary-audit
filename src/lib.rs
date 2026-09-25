//! meshcheck — combinatorial topology auditor for directed triangle soups.
//!
//! Two audit modes share the same stage pipeline:
//!
//! * [`AuditMode::Closed`] (via [`analyze`], the default) proves the input is
//!   a *combinatorially watertight* orientable **closed** triangle mesh —
//!   every undirected edge is used by exactly two faces with opposite
//!   directions, the faces around every vertex form a single fan (a cyclic
//!   link), and the face complex is connected.
//! * [`AuditMode::Boundary`] (via [`analyze_with_boundary`]) additionally
//!   accepts a compact surface with holes: an edge may be used by exactly one
//!   face (a boundary edge). Two faces sharing an edge must still traverse it
//!   in opposite directions, and three or more uses still fail. Every vertex
//!   link must be one simple path (boundary vertex) or one cycle (interior
//!   vertex); forks, multiple sectors and isolated vertices still fail. On
//!   success the induced, mutually disjoint boundary loops are reported.
//!
//! Neither mode inspects coordinates and therefore neither can detect
//! geometric self-intersection or degenerate embedding.

use std::collections::{BTreeMap, BTreeSet};

/// Inclusive bounds mandated by the input specification.
pub const MIN_VERTICES: usize = 4;
pub const MAX_VERTICES: usize = 500;
pub const MIN_FACES: usize = 4;
pub const MAX_FACES: usize = 2000;

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

/// Which class of surfaces an audit accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AuditMode {
    /// Every edge must be used by exactly two oppositely directed faces.
    #[default]
    Closed,
    /// Edges used by exactly one face are accepted as boundary edges.
    Boundary,
}

/// The kind of failure found while auditing undirected edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeFault {
    /// The edge belongs to only one directed face (a mesh boundary);
    /// legal only in [`AuditMode::Boundary`].
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
    /// The faces around this vertex do not form exactly one cyclic fan.
    VertexFailed {
        id: String,
        sectors: usize,
    },
    /// Two or more closed components exist; this is the smallest vertex id
    /// outside the component containing the smallest vertex overall.
    ComponentFailed {
        id: String,
    },
    Ok {
        vertices: usize,
        edges: usize,
        faces: usize,
        euler: i64,
        genus: u32,
    },
    /// Boundary mode succeeded: a connected orientable compact surface with
    /// `boundary.len()` holes. `boundary` is the canonical list of boundary
    /// loops; the loops are mutually vertex- and edge-disjoint.
    BoundaryOk {
        vertices: usize,
        edges: usize,
        faces: usize,
        euler: i64,
        genus: u32,
        boundary: Vec<Vec<String>>,
    },
    /// Boundary mode passed the combinatorial stages but the surface
    /// identity χ = 2 − 2g − b has no non-negative integer solution for g.
    /// The audit never claims success in that case.
    TopologyFailed {
        vertices: usize,
        edges: usize,
        faces: usize,
        euler: i64,
        boundary_count: usize,
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

/// undirected edge -> counts of traversal in each direction (lexicographic
/// then reverse), in lexicographic undirected-edge order.
fn edge_uses(mesh: &Mesh) -> BTreeMap<(String, String), [usize; 2]> {
    // undirected edge -> counts of traversal in each direction
    let mut seen: BTreeMap<(String, String), [usize; 2]> = BTreeMap::new();
    for face in &mesh.faces {
        let pairs = [
            (face[0].clone(), face[1].clone()),
            (face[1].clone(), face[2].clone()),
            (face[2].clone(), face[0].clone()),
        ];
        for (u, v) in pairs {
            let key = undirected(&u, &v);
            let dir = if u == key.0 { 0 } else { 1 };
            let entry = seen.entry(key).or_insert([0, 0]);
            entry[dir] += 1;
        }
    }
    seen
}

/// Stage 1: every undirected edge must be used by exactly two directed
/// faces, and the two uses must traverse it in opposite directions.
///
/// The witness is the lexicographically smallest faulty undirected edge
/// (ordered by smaller endpoint id, then larger).
pub fn audit_edges(mesh: &Mesh) -> Result<(), EdgeFailure> {
    audit_edges_mode(mesh, AuditMode::Closed)
}

/// Stage 1 with an explicit mode. In [`AuditMode::Boundary`], an edge used
/// by exactly one directed face is legal; same-direction pairs and three or
/// more uses still fail with the original edge-level priority.
fn audit_edges_mode(mesh: &Mesh, mode: AuditMode) -> Result<(), EdgeFailure> {
    for ((a, b), dirs) in edge_uses(mesh) {
        let uses = dirs[0] + dirs[1];
        let fault = if uses == 1 {
            if mode == AuditMode::Boundary {
                continue; // legal hole edge
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

/// Stage 2: the faces incident on each vertex must form exactly one cyclic
/// fan — equivalently, the vertex link is a single directed cycle.
///
/// For a directed face `(x, y, z)` the link edge at vertex `v` points from
/// `v`'s successor to `v`'s predecessor (each face corner goes around the
/// vertex consistently with the face orientation). Two faces sharing an
/// edge glue the matching link endpoints; the link therefore decomposes
/// into disjoint cycles, one cycle per surface sector ("fan") at `v`.
pub fn audit_vertex_sectors(mesh: &Mesh) -> Result<(), (String, usize)> {
    audit_vertex_sectors_mode(mesh, AuditMode::Closed)
}

/// Stage 2 with an explicit mode. In [`AuditMode::Boundary`] the single
/// link component may also be a simple path (the vertex lies on one hole):
/// a link component is accepted when every one of its nodes has in-degree
/// and out-degree at most one and the component is either a directed cycle
/// (one sector, interior vertex) or a directed path with exactly two degree-1
/// ends (boundary vertex). Forks, two sectors and isolated vertices fail.
fn audit_vertex_sectors_mode(mesh: &Mesh, mode: AuditMode) -> Result<(), (String, usize)> {
    // vertex -> (successor -> predecessor) adjacency of link edges.
    // After a passed edge stage every successor key is unique; a collision
    // (broken precondition) is handled by auditing the link as a general
    // directed graph below rather than by panicking.
    let mut link: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    // Undirected version of the same link: vertex -> (node -> neighbours),
    // for weak-component counting.
    let mut adj: BTreeMap<String, BTreeMap<String, BTreeSet<String>>> = BTreeMap::new();
    for v in &mesh.vertices {
        link.insert(v.clone(), BTreeMap::new());
        adj.insert(v.clone(), BTreeMap::new());
    }
    for face in &mesh.faces {
        let corners = [(0usize, 1usize, 2usize), (1, 2, 0), (2, 0, 1)];
        for (vi, si, pi) in corners {
            let v = &face[vi];
            let successor = &face[si];
            let predecessor = &face[pi];
            link.get_mut(v)
                .expect("vertex already registered")
                .insert(successor.clone(), predecessor.clone());
            let graph = adj.get_mut(v).expect("vertex already registered");
            graph
                .entry(successor.clone())
                .or_default()
                .insert(predecessor.clone());
            graph
                .entry(predecessor.clone())
                .or_default()
                .insert(successor.clone());
        }
    }

    for v in &mesh.vertices {
        let link = &link[v];
        // Weak connected components of the link graph (edges connect each
        // successor to its predecessor). For a fan it is one directed
        // cycle; for a pinched vertex the link is two cycles; for a
        // boundary vertex it is one path.
        let graph = &adj[v];
        let nodes: BTreeSet<String> = graph.keys().cloned().collect();
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
                if let Some(neighbours) = graph.get(&cur) {
                    for next in neighbours {
                        stack.push(next.clone());
                    }
                }
            }
        }
        if sectors != 1 {
            // Isolated declared vertex: no link nodes at all (zero sectors).
            return Err((v.clone(), sectors));
        }

        if mode == AuditMode::Boundary {
            // The single component must be a simple path or a cycle: at most
            // one outgoing and one incoming link edge per node …
            let mut indegree: BTreeMap<String, usize> =
                nodes.iter().map(|node| (node.clone(), 0)).collect();
            for predecessor in link.values() {
                *indegree.get_mut(predecessor).expect("link node present") += 1;
            }
            let mut degree_one_ends = 0usize;
            for node in &nodes {
                let out = if link.contains_key(node) { 1 } else { 0 };
                let inn = indegree[node];
                if out > 1 || inn > 1 {
                    // Forked link (defense in depth; the edge stage already
                    // excludes this for triangle meshes).
                    return Err((v.clone(), sectors));
                }
                if out + inn == 1 {
                    degree_one_ends += 1;
                }
            }
            // … and a path has exactly two ends, a cycle none.
            if degree_one_ends != 0 && degree_one_ends != 2 {
                return Err((v.clone(), sectors));
            }
        }
    }
    Ok(())
}

struct UnionFind {
    parent: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        UnionFind {
            parent: (0..n).collect(),
        }
    }
    fn find(&mut self, x: usize) -> usize {
        if self.parent[x] != x {
            self.parent[x] = self.find(self.parent[x]);
        }
        self.parent[x]
    }
    fn union(&mut self, a: usize, b: usize) {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra != rb {
            self.parent[ra] = rb;
        }
    }
}

/// Stage 3: all faces must belong to one connected component (faces sharing
/// an edge are adjacent). Witness: smallest vertex id outside the component
/// holding the smallest vertex overall.
pub fn audit_connected(mesh: &Mesh) -> Result<(), String> {
    let index: BTreeMap<&str, usize> = mesh
        .vertices
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();

    let mut uf = UnionFind::new(mesh.vertices.len());
    for face in &mesh.faces {
        let i0 = index[face[0].as_str()];
        let i1 = index[face[1].as_str()];
        let i2 = index[face[2].as_str()];
        uf.union(i0, i1);
        uf.union(i1, i2);
        uf.union(i0, i2);
    }

    let root = uf.find(0); // vertices are sorted: index 0 is the smallest id
    for (i, id) in mesh.vertices.iter().enumerate() {
        if uf.find(i) != root {
            return Err(id.clone());
        }
    }
    Ok(())
}

/// Number of distinct undirected edges in the parsed mesh.
pub fn edge_count(mesh: &Mesh) -> usize {
    let mut edges: BTreeSet<(String, String)> = BTreeSet::new();
    for face in &mesh.faces {
        edges.insert(undirected(&face[0], &face[1]));
        edges.insert(undirected(&face[1], &face[2]));
        edges.insert(undirected(&face[2], &face[0]));
    }
    edges.len()
}

/// Run the full fixed-priority audit:
/// parse → edges → vertex sectors → global connectivity → invariants.
pub fn analyze(input: &str) -> Report {
    analyze_mode(input, AuditMode::Closed)
}

/// Run the audit in [`AuditMode::Boundary`]: single-use edges are legal
/// holes, vertex links may be simple paths, and a successful report carries
/// the canonical boundary loops and the genus solved from
/// χ = 2 − 2g − b.
pub fn analyze_with_boundary(input: &str) -> Report {
    analyze_mode(input, AuditMode::Boundary)
}

fn analyze_mode(input: &str, mode: AuditMode) -> Report {
    let mesh = match parse(input) {
        Ok(mesh) => mesh,
        Err(reject) => return Report::Rejected(reject),
    };

    if let Err(failure) = audit_edges_mode(&mesh, mode) {
        return Report::EdgeFailed(failure);
    }
    if let Err((id, sectors)) = audit_vertex_sectors_mode(&mesh, mode) {
        return Report::VertexFailed { id, sectors };
    }
    if let Err(id) = audit_connected(&mesh) {
        return Report::ComponentFailed { id };
    }

    let v = mesh.vertices.len();
    let f = mesh.faces.len();
    let e = edge_count(&mesh);
    let euler = v as i64 - e as i64 + f as i64;

    if mode == AuditMode::Closed {
        // For a closed connected orientable surface: chi = 2 - 2g.
        // Every passed audit yields 2g = 2 - chi >= 0; the clamp is pure
        // defense in depth against an internal invariant violation.
        let genus = (((2 - euler) / 2).max(0)) as u32;
        return Report::Ok {
            vertices: v,
            edges: e,
            faces: f,
            euler,
            genus,
        };
    }

    // Boundary mode: the single-face edges induce the boundary loops. Their
    // combinatorial integrity was established by the stages above; any
    // inconsistency below means the identity cannot be trusted.
    let boundary = match trace_boundary_loops(&mesh) {
        Some(loops) => loops,
        None => {
            return Report::TopologyFailed {
                vertices: v,
                edges: e,
                faces: f,
                euler,
                boundary_count: 0,
            }
        }
    };
    let b = boundary.len() as i64;

    match boundary_genus(euler, b) {
        Some(genus) => Report::BoundaryOk {
            vertices: v,
            edges: e,
            faces: f,
            euler,
            genus,
            boundary,
        },
        None => Report::TopologyFailed {
            vertices: v,
            edges: e,
            faces: f,
            euler,
            boundary_count: boundary.len(),
        },
    }
}

/// Solve χ = 2 − 2g − b for a non-negative integer genus. Returns `None`
/// unless 2 − b − χ is a non-negative even integer.
fn boundary_genus(euler: i64, b: i64) -> Option<u32> {
    let twice = 2 - b - euler;
    if twice < 0 || twice % 2 != 0 {
        return None;
    }
    let g = twice / 2;
    u32::try_from(g).ok()
}

/// Trace the boundary loops induced by edges used by exactly one face.
///
/// A boundary edge is taken in the direction its sole face traverses it.
/// After a passed boundary audit these directed edges partition into
/// disjoint simple cycles (each boundary vertex link is one path, so each
/// such vertex has exactly one outgoing and one incoming boundary edge).
/// Each loop is rotated to begin at its smallest vertex id (traversal
/// direction is never reversed), and the loops are sorted by first vertex.
///
/// `None` reports any structural surprise (a non-loop component, a repeated
/// visit or a loop shorter than three edges); the caller then refuses the
/// success verdict rather than publishing bad loops.
fn trace_boundary_loops(mesh: &Mesh) -> Option<Vec<Vec<String>>> {
    // directed boundary edge (u -> v) -> v
    let mut outgoing: BTreeMap<String, String> = BTreeMap::new();
    for ((a, b), dirs) in edge_uses(mesh) {
        if dirs[0] + dirs[1] != 1 {
            continue;
        }
        if dirs[0] == 1 {
            outgoing.insert(a, b);
        } else {
            outgoing.insert(b, a);
        }
    }

    let mut remaining: BTreeSet<(String, String)> = outgoing
        .iter()
        .map(|(u, v)| (u.clone(), v.clone()))
        .collect();
    let mut loops: Vec<Vec<String>> = Vec::new();

    while let Some((start, _)) = remaining.iter().next().cloned() {
        let mut cycle: Vec<String> = vec![start.clone()];
        let mut cur = start.clone();
        loop {
            let next = outgoing.get(&cur)?.clone();
            // The edge was already consumed in this or an earlier loop:
            // the boundary edges do not form disjoint cycles.
            if !remaining.remove(&(cur.clone(), next.clone())) {
                return None;
            }
            if next == start {
                break;
            }
            // Revisiting an interior vertex closes on the wrong point.
            if cycle.contains(&next) {
                return None;
            }
            cycle.push(next.clone());
            cur = next;
        }
        if cycle.len() < 3 {
            return None;
        }

        // Rotate so the smallest vertex id leads; the induced direction is
        // preserved. It is unique because a simple loop has distinct ids.
        let min_pos = cycle
            .iter()
            .enumerate()
            .min_by(|(_, x), (_, y)| x.cmp(y))
            .map(|(i, _)| i)?;
        cycle.rotate_left(min_pos);
        loops.push(cycle);
    }

    // Deterministic order: by leading vertex, then the full vertex sequence.
    loops.sort();
    Some(loops)
}

#[cfg(test)]
mod tests {
    use super::*;

    // chi = 2 - 2g - b  must yield a non-negative integer g; a failed solve
    // is the caller's signal never to publish a success verdict.
    #[test]
    fn genus_identity_examples() {
        assert_eq!(boundary_genus(2, 0), Some(0)); // sphere
        assert_eq!(boundary_genus(0, 0), Some(1)); // torus
        assert_eq!(boundary_genus(-2, 0), Some(2)); // genus 2
        assert_eq!(boundary_genus(1, 1), Some(0)); // disc
        assert_eq!(boundary_genus(0, 2), Some(0)); // annulus
        assert_eq!(boundary_genus(-1, 1), Some(1)); // punctured torus
        assert_eq!(boundary_genus(-3, 1), Some(2)); // twice-punctured genus-2
    }

    #[test]
    fn genus_identity_rejects_impossible_values() {
        assert_eq!(boundary_genus(3, 0), None); // chi > 2
        assert_eq!(boundary_genus(1, 0), None); // 2 - chi odd
        assert_eq!(boundary_genus(-1, 0), None); // closed, odd chi
        assert_eq!(boundary_genus(-2, 1), None); // 3 - chi odd
        assert_eq!(boundary_genus(2, 3), None); // negative genus
    }

    #[test]
    fn mode_default_is_closed() {
        assert_eq!(AuditMode::default(), AuditMode::Closed);
    }
}
