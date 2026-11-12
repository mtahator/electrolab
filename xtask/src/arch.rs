//! Dependency rules between crates (Draft 2.2 §6.2, §6.3, §8.6; technical-structure.md §3).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use cargo_metadata::{DependencyKind, Metadata};

use crate::layout::{PURE_FOUNDATION, Place, Role, allowed_engines, is_os_adapter_part, place_of};

/// One workspace crate and its workspace dependencies (normal and build; dev-dependencies are free).
pub struct Node {
    pub name: String,
    pub place: Place,
    pub deps: Vec<String>,
}

/// Check every workspace crate against the rules. Returns human-readable violations.
pub fn check(metadata: &Metadata, root: &Path) -> Vec<String> {
    let names: Vec<String> = metadata.packages.iter().map(|p| p.name.clone()).collect();
    let nodes: Vec<Node> = metadata
        .packages
        .iter()
        .map(|p| {
            let dir = p
                .manifest_path
                .parent()
                .map_or_else(PathBuf::new, |d| d.as_std_path().to_path_buf());
            Node {
                name: p.name.clone(),
                place: place_of(&dir, root),
                deps: p
                    .dependencies
                    .iter()
                    .filter(|d| d.kind != DependencyKind::Development)
                    .filter(|d| names.contains(&d.name))
                    .map(|d| d.name.clone())
                    .collect(),
            }
        })
        .collect();
    check_nodes(&nodes)
}

/// The rules, on plain data so they can be tested without Cargo.
pub fn check_nodes(nodes: &[Node]) -> Vec<String> {
    let by_name: HashMap<&str, &Place> =
        nodes.iter().map(|n| (n.name.as_str(), &n.place)).collect();
    let mut out = Vec::new();
    for node in nodes {
        if let Place::Engine { engine, .. } = &node.place
            && allowed_engines(engine).is_none()
        {
            out.push(format!(
                "{}: unknown engine folder `engines/{engine}`",
                node.name
            ));
        }
        for dep in &node.deps {
            let Some(target) = by_name.get(dep.as_str()) else {
                continue;
            };
            if let Some(reason) = forbidden(&node.place, target) {
                out.push(format!("{} -> {}: {}", node.name, dep, reason));
            }
        }
    }
    out.sort();
    out
}

/// Why `from` may not depend on `to`, or `None` if it may.
fn forbidden(from: &Place, to: &Place) -> Option<String> {
    match (from, to) {
        (_, Place::Tooling) | (Place::Tooling | Place::Host, _) => None,
        (_, Place::Host) => Some("nothing may depend on a host".into()),

        // Foundation: never on engines; OS adapters are chosen only by hosts.
        (Place::Foundation { .. }, Place::Engine { .. }) => {
            Some("foundation may not depend on an engine".into())
        }
        (Place::Foundation { part: a }, Place::Foundation { part: b }) => (is_os_adapter_part(b)
            && !is_os_adapter_part(a))
        .then(|| "only hosts choose OS adapters; depend on the `ep-os` ports".into()),

        // Engine -> foundation.
        (Place::Engine { role, .. }, Place::Foundation { part }) => {
            if is_os_adapter_part(part) {
                return Some("engines use OS ports from `ep-os`, never an OS adapter crate".into());
            }
            let pure = matches!(role, Role::Contract | Role::Core);
            (pure && !PURE_FOUNDATION.contains(&part.as_str())).then(|| {
                format!("contract and core crates may use only {PURE_FOUNDATION:?} from foundation")
            })
        }

        // Engine -> engine.
        (
            Place::Engine {
                engine: e1,
                role: r1,
            },
            Place::Engine {
                engine: e2,
                role: r2,
            },
        ) => {
            if e1 == e2 {
                return same_engine(*r1, *r2);
            }
            if *r2 != Role::Contract {
                return Some(format!(
                    "engines know each other only through contracts; this is `{e2}` {r2:?}"
                ));
            }
            if *r1 == Role::Core {
                return Some("a core depends only on its own contract".into());
            }
            let allowed = allowed_engines(e1).unwrap_or(&[]);
            (!allowed.contains(&e2.as_str()))
                .then(|| format!("`{e1}` may not use `{e2}` (allowed order of §6.3)"))
        }
    }
}

/// Rules inside one engine: contract <- core <- adapters <- facade <- cli.
fn same_engine(from: Role, to: Role) -> Option<String> {
    let ok = match from {
        Role::Contract => false,
        Role::Core => to == Role::Contract,
        Role::Adapter => matches!(to, Role::Contract | Role::Core),
        Role::Facade => matches!(to, Role::Contract | Role::Core | Role::Adapter),
        Role::Cli => matches!(to, Role::Contract | Role::Facade),
    };
    (!ok).then(|| format!("inside an engine, {from:?} may not depend on {to:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine(name: &str, engine: &str, role: Role, deps: &[&str]) -> Node {
        Node {
            name: name.into(),
            place: Place::Engine {
                engine: engine.into(),
                role,
            },
            deps: deps.iter().map(|d| (*d).to_string()).collect(),
        }
    }

    fn foundation(name: &str, part: &str, deps: &[&str]) -> Node {
        Node {
            name: name.into(),
            place: Place::Foundation { part: part.into() },
            deps: deps.iter().map(|d| (*d).to_string()).collect(),
        }
    }

    fn host(name: &str, deps: &[&str]) -> Node {
        Node {
            name: name.into(),
            place: Place::Host,
            deps: deps.iter().map(|d| (*d).to_string()).collect(),
        }
    }

    /// A correct graph shaped like Phase 1.
    fn base() -> Vec<Node> {
        vec![
            foundation("ep-types", "types", &[]),
            foundation("ep-store", "store", &["ep-types"]),
            foundation("ep-os", "os", &["ep-types"]),
            foundation("ep-os-windows", "os-windows", &["ep-os"]),
            engine(
                "ep-document-contract",
                "document",
                Role::Contract,
                &["ep-types"],
            ),
            engine(
                "ep-document-core",
                "document",
                Role::Core,
                &["ep-document-contract", "ep-types"],
            ),
            engine(
                "ep-document-adapter-pdfium",
                "document",
                Role::Adapter,
                &["ep-document-contract"],
            ),
            engine(
                "ep-document",
                "document",
                Role::Facade,
                &[
                    "ep-document-core",
                    "ep-document-adapter-pdfium",
                    "ep-store",
                    "ep-os",
                ],
            ),
            engine("ep-document-cli", "document", Role::Cli, &["ep-document"]),
            engine(
                "ep-knowledge-contract",
                "knowledge",
                Role::Contract,
                &["ep-types"],
            ),
            engine(
                "ep-knowledge",
                "knowledge",
                Role::Facade,
                &["ep-document-contract"],
            ),
            host(
                "ep-reader",
                &["ep-document", "ep-knowledge", "ep-os-windows"],
            ),
        ]
    }

    fn with(extra: Node) -> Vec<String> {
        let mut g = base();
        g.push(extra);
        check_nodes(&g)
    }

    #[test]
    fn clean_graph_passes() {
        assert_eq!(check_nodes(&base()), Vec::<String>::new());
    }

    #[test]
    fn using_another_engines_facade_or_core_fails() {
        let v = with(engine(
            "ep-understanding",
            "understanding",
            Role::Facade,
            &["ep-document-core"],
        ));
        assert_eq!(v.len(), 1, "{v:?}");
        let v = with(engine(
            "ep-understanding",
            "understanding",
            Role::Facade,
            &["ep-document"],
        ));
        assert_eq!(v.len(), 1, "{v:?}");
    }

    #[test]
    fn breaking_the_engine_order_fails() {
        // Document may not use Knowledge: Knowledge already uses Document (§6.3).
        let v = with(engine(
            "ep-document-x",
            "document",
            Role::Facade,
            &["ep-knowledge-contract"],
        ));
        assert_eq!(v.len(), 1, "{v:?}");
    }

    #[test]
    fn impure_core_fails() {
        let v = with(engine(
            "ep-knowledge-core",
            "knowledge",
            Role::Core,
            &["ep-store"],
        ));
        assert_eq!(v.len(), 1, "{v:?}");
    }

    #[test]
    fn core_using_another_contract_fails() {
        let v = with(engine(
            "ep-knowledge-core",
            "knowledge",
            Role::Core,
            &["ep-document-contract"],
        ));
        assert_eq!(v.len(), 1, "{v:?}");
    }

    #[test]
    fn engine_depending_on_os_adapter_fails() {
        let v = with(engine(
            "ep-workspace",
            "workspace",
            Role::Facade,
            &["ep-os-windows"],
        ));
        assert_eq!(v.len(), 1, "{v:?}");
    }

    #[test]
    fn foundation_depending_on_engine_fails() {
        let v = with(foundation("ep-bus", "bus", &["ep-document-contract"]));
        assert_eq!(v.len(), 1, "{v:?}");
    }

    #[test]
    fn depending_on_a_host_fails() {
        let v = with(engine(
            "ep-workspace",
            "workspace",
            Role::Facade,
            &["ep-reader"],
        ));
        assert_eq!(v.len(), 1, "{v:?}");
    }

    #[test]
    fn unknown_engine_folder_fails() {
        let v = with(engine("ep-misc", "misc", Role::Facade, &[]));
        assert_eq!(v.len(), 1, "{v:?}");
    }

    #[test]
    fn contract_depending_on_own_core_fails() {
        let v = with(engine(
            "ep-document-contract2",
            "document",
            Role::Contract,
            &["ep-document-core"],
        ));
        assert_eq!(v.len(), 1, "{v:?}");
    }
}
