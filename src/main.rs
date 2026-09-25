//! Command-line front end for the meshcheck topology auditor.
//!
//! Usage:
//!   meshcheck [-b|--boundary] [FILE]   audit FILE; no FILE (or `-`) = stdin
//!   meshcheck --help                   show help

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
        } => format!("ok V={vertices} E={edges} F={faces} chi={euler} genus={genus}"),
        Report::BoundaryOk {
            vertices,
            edges,
            faces,
            euler,
            genus,
            boundary,
        } => {
            // The loop list is the same one the structured report carries;
            // the text view only formats it.
            let mut out = format!(
                "ok-boundary V={vertices} E={edges} F={faces} chi={euler} genus={genus} holes={}",
                boundary.len()
            );
            for loop_ in boundary {
                out.push_str("\nboundary ");
                out.push_str(&loop_.join("-"));
            }
            out
        }
        Report::TopologyFailed {
            vertices,
            edges,
            faces,
            euler,
            boundary_count,
        } => format!(
            "topology-fail V={vertices} E={edges} F={faces} chi={euler} holes={boundary_count}"
        ),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("meshcheck — combinatorial topology auditor for triangle meshes");
        println!();
        println!("Usage: meshcheck [OPTIONS] [FILE]");
        println!("  FILE  mesh document; omit or use '-' to read standard input");
        println!();
        println!("Options:");
        println!("  -b, --boundary   audit a surface with holes: edges used by one");
        println!("                   face are legal boundary edges; success reports");
        println!("                   the canonical boundary loops");
        println!();
        println!("Document grammar (one record per line, '#' starts a comment):");
        println!("  v <id>                      declare a vertex");
        println!("  f <id> <id> <id>            directed triangle (CCW order)");
        println!();
        println!("Audit priority: edge conditions, then per-vertex fan sectors,");
        println!("then global face connectivity. Exit code: 0 valid, 1 rejected,");
        println!("2 edge failure, 3 vertex failure, 4 disconnected, 5 surface");
        println!("identity inconsistent, 3 usage error.");
        return ExitCode::from(3);
    }

    let mut mode = AuditMode::Closed;
    let mut file: Option<&str> = None;
    for arg in &args {
        match arg {
            s if s == "-b" || s == "--boundary" => mode = AuditMode::Boundary,
            s if s.starts_with('-') && s != "-" => {
                eprintln!("meshcheck: unknown option '{s}'");
                return ExitCode::from(3);
            }
            path => {
                if file.is_some() {
                    eprintln!("meshcheck: expected at most one FILE argument");
                    return ExitCode::from(3);
                }
                file = Some(path);
            }
        }
    }

    let input = match file {
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

    let report = match mode {
        AuditMode::Closed => meshcheck::analyze(&input),
        AuditMode::Boundary => meshcheck::analyze_with_boundary(&input),
    };
    println!("{}", render(&report));
    match report {
        Report::Ok { .. } | Report::BoundaryOk { .. } => ExitCode::SUCCESS,
        Report::Rejected(_) => ExitCode::from(1),
        Report::EdgeFailed(_) => ExitCode::from(2),
        Report::VertexFailed { .. } => ExitCode::from(3),
        Report::ComponentFailed { .. } => ExitCode::from(4),
        Report::TopologyFailed { .. } => ExitCode::from(5),
    }
}
