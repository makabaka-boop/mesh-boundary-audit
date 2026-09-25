//! Command-line front end for the meshcheck topology auditor.
//!
//! Usage:
//!   meshcheck [--boundary] [FILE]  audit FILE; with no FILE (or `-`) read stdin
//!   meshcheck --help               show help

use std::io::Read;
use std::process::ExitCode;

use meshcheck::{AuditMode, EdgeFault, Reject, Report};

fn reject_kind(r: &Reject) -> &'static str {
    match r {
        Reject::UnknownLine { .. } => "unknown_line",
        Reject::BadVertex { .. } => "bad_vertex",
        Reject::BadFaceArity { .. } => "bad_face_arity",
        Reject::BadId { .. } => "bad_id",
        Reject::RepeatedVertex { .. } => "repeated_vertex",
        Reject::DuplicateVertex { .. } => "duplicate_vertex",
        Reject::UnknownVertex { .. } => "unknown_vertex",
        Reject::DuplicateFace { .. } => "duplicate_face",
        Reject::BadVertexCount { .. } => "bad_vertex_count",
        Reject::BadFaceCount { .. } => "bad_face_count",
    }
}

fn append_basic(
    out: &mut String,
    vertices: usize,
    edges: usize,
    faces: usize,
    euler: i64,
) {
    out.push_str(&format!(
        " V={vertices} E={edges} F={faces} chi={euler}"
    ));
}

fn render(report: &Report) -> String {
    match report {
        Report::Rejected(r) => {
            let mut out = format!("reject kind={}", reject_kind(r));
            if let Some(line) = r.line() {
                out.push_str(&format!(" line={line}"));
            }
            match r {
                Reject::BadId { id, .. }
                | Reject::DuplicateVertex { id, .. }
                | Reject::UnknownVertex { id, .. } => {
                    out.push_str(&format!(" id={id}"));
                }
                Reject::BadVertexCount { count } => {
                    out.push_str(&format!(" count={count}"));
                }
                Reject::BadFaceCount { count } => {
                    out.push_str(&format!(" count={count}"));
                }
                _ => {}
            }
            out
        }
        Report::EdgeFailed(f) => {
            let fault = match f.fault {
                EdgeFault::Boundary => "boundary",
                EdgeFault::Misoriented => "misoriented",
                EdgeFault::Nonmanifold => "nonmanifold",
            };
            format!(
                "edge-fail edge={a}-{b} uses={uses} fault={fault}",
                a = f.a,
                b = f.b,
                uses = f.uses
            )
        }
        Report::VertexFailed { id, sectors } => {
            format!("vertex-fail vertex={id} sectors={sectors}")
        }
        Report::ComponentFailed { id } => {
            format!("component-fail vertex={id}")
        }
        Report::Ok {
            vertices,
            edges,
            faces,
            euler,
            genus,
        } => {
            let mut out = "ok".to_string();
            append_basic(&mut out, *vertices, *edges, *faces, *euler);
            out.push_str(&format!(" genus={genus}"));
            out
        }
        Report::BoundaryOk {
            vertices,
            edges,
            faces,
            euler,
            genus,
            boundary_loops,
        } => {
            let mut out = "boundary-ok".to_string();
            append_basic(&mut out, *vertices, *edges, *faces, *euler);
            out.push_str(&format!(
                " boundaries={} genus={genus}",
                boundary_loops.len()
            ));
            for loop_vertices in boundary_loops {
                out.push_str(" loop=");
                out.push_str(&loop_vertices.join(","));
            }
            out
        }
        Report::InvariantFailed {
            vertices,
            edges,
            faces,
            euler,
            boundary_loops,
        } => {
            let mut out = "invariant-fail".to_string();
            append_basic(&mut out, *vertices, *edges, *faces, *euler);
            out.push_str(&format!(" boundaries={}", boundary_loops.len()));
            out
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("meshcheck — combinatorial topology auditor for triangle meshes");
        println!();
        println!("Usage: meshcheck [--boundary] [FILE]");
        println!("  FILE        mesh document; omit or use '-' to read standard input");
        println!("  --boundary  accept one-face edges and list their boundary loops");
        println!("  -b          shorthand for --boundary");
        println!();
        println!("Document grammar (one record per line, '#' starts a comment):");
        println!("  v <id>                      declare a vertex");
        println!("  f <id> <id> <id>            directed triangle (CCW order)");
        println!();
        println!("Default mode is closed: every edge must have two opposite face uses.");
        println!("Boundary mode also permits one use; three or more or same-direction");
        println!("edges still fail. Audit priority: edge conditions, per-vertex links,");
        println!("then face connectivity. Exit code: 0 valid, 1 rejected, 2 edge");
        println!("failure, 3 vertex failure, 4 disconnected, 5 invariant failure,");
        println!("3 usage error.");
        return ExitCode::from(3);
    }

    let mut mode = AuditMode::Closed;
    let mut path: Option<&str> = None;
    for arg in &args {
        match arg.as_str() {
            "--boundary" | "-b" => mode = AuditMode::Boundary,
            "-" if path.is_none() => path = Some("-"),
            arg if arg.starts_with('-') && arg != "-" => {
                eprintln!("meshcheck: unknown option: {arg}");
                return ExitCode::from(3);
            }
            _ if path.is_none() => path = Some(arg),
            _ => {
                eprintln!("meshcheck: expected at most one FILE argument");
                return ExitCode::from(3);
            }
        }
    }

    let input = match path {
        None | Some("-") => {
            let mut buf = String::new();
            if let Err(e) = std::io::stdin().read_to_string(&mut buf) {
                eprintln!("meshcheck: failed to read stdin: {e}");
                return ExitCode::from(3);
            }
            buf
        }
        Some(path) => match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("meshcheck: failed to read {path}: {e}");
                return ExitCode::from(3);
            }
        },
    };

    let report = meshcheck::analyze_with(&input, mode);
    println!("{}", render(&report));
    match report {
        Report::Ok { .. } | Report::BoundaryOk { .. } => ExitCode::SUCCESS,
        Report::Rejected(_) => ExitCode::from(1),
        Report::EdgeFailed(_) => ExitCode::from(2),
        Report::VertexFailed { .. } => ExitCode::from(3),
        Report::ComponentFailed { .. } => ExitCode::from(4),
        Report::InvariantFailed { .. } => ExitCode::from(5),
    }
}
