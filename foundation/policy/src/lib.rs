//! Permissions and the data boundary as one pure function (Draft 2.2 §13.2, §16.4, §16.5).
//!
//! `decide(request, settings) -> Allow | NeedsApproval | Deny`, always citing the rule used.
//! Deterministic, no I/O: the bus calls it before every handler, and the model router, network,
//! export and (later) sandbox call it before acting.
#![deny(missing_docs)]

use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// What kind of operation (§16.4 permission levels).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    /// Read papers and project files.
    Read,
    /// Create notes, notebooks or files in the project.
    Create,
    /// Run code or simulations locally.
    Execute,
    /// Run on HPC, SSH or cloud machines.
    RemoteExecute,
    /// Send data to an external model or service.
    ExternalApi,
    /// Delete, overwrite or publish.
    Destructive,
}

/// The kind of data an operation touches (§16.5).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum DataClass {
    /// A published paper.
    PublicPaper,
    /// A dataset the user marked confidential.
    ConfidentialDataset,
    /// The user's own unpublished ideas, notes and drafts.
    UnpublishedIdea,
    /// Results of the user's experiments.
    ExperimentResults,
    /// Embeddings computed from documents.
    DocumentEmbeddings,
    /// Human or genetic data.
    HumanOrGenetic,
    /// Anything the user marked "local only".
    LocalOnly,
}

/// Where the data goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Destination {
    /// Stays on this computer.
    Device,
    /// Our own service (sync, later).
    OurServer,
    /// A third-party model or service.
    External,
}

/// Extra detail that changes the decision.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Medium {
    /// Text or structured data.
    #[default]
    Text,
    /// Page images (vision reading).
    Images,
}

/// One question to the policy.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Request {
    /// Operation.
    pub operation: Operation,
    /// Every data class involved (the strictest wins).
    pub data: BTreeSet<DataClass>,
    /// Where it goes.
    pub destination: Destination,
    /// Text or images.
    #[serde(default)]
    pub medium: Medium,
}

/// The user's and project's choices that the rules read.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Settings {
    /// Data classes the user has agreed may go to external models and services.
    pub external_consent: BTreeSet<DataClass>,
    /// Data classes the user has agreed may go to our server.
    pub server_consent: BTreeSet<DataClass>,
    /// The user has agreed that page images may be sent to a remote vision model.
    pub remote_vision_consent: bool,
    /// Creating files needs approval (§16.4 "per user setting").
    pub create_needs_approval: bool,
    /// Local execution approved for the current workflow.
    pub execute_approved: bool,
}

/// The policy's answer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum Decision {
    /// Go ahead.
    Allow {
        /// Rule that allowed it.
        rule: String,
    },
    /// Ask the user first.
    NeedsApproval {
        /// Rule that requires approval.
        rule: String,
        /// What to tell the user.
        reason: String,
    },
    /// Never.
    Deny {
        /// Rule that forbids it.
        rule: String,
        /// Why.
        reason: String,
    },
}

impl Decision {
    fn rank(&self) -> u8 {
        match self {
            Self::Allow { .. } => 0,
            Self::NeedsApproval { .. } => 1,
            Self::Deny { .. } => 2,
        }
    }

    /// The rule cited.
    #[must_use]
    pub fn rule(&self) -> &str {
        match self {
            Self::Allow { rule } | Self::NeedsApproval { rule, .. } | Self::Deny { rule, .. } => {
                rule
            }
        }
    }

    /// Whether the operation may run now.
    #[must_use]
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allow { .. })
    }
}

fn allow(rule: &str) -> Decision {
    Decision::Allow { rule: rule.into() }
}
fn ask(rule: &str, reason: impl Into<String>) -> Decision {
    Decision::NeedsApproval {
        rule: rule.into(),
        reason: reason.into(),
    }
}
fn deny(rule: &str, reason: impl Into<String>) -> Decision {
    Decision::Deny {
        rule: rule.into(),
        reason: reason.into(),
    }
}

/// The policy. Pure and deterministic: the same request and settings always give the same
/// decision. The strictest applicable rule wins; ties keep the first rule found.
#[must_use]
pub fn decide(req: &Request, s: &Settings) -> Decision {
    let mut out = operation_rule(req.operation, s);
    let mut consider = |d: Decision| {
        if d.rank() > out.rank() {
            out = d;
        }
    };
    for &class in &req.data {
        consider(boundary_rule(class, req.destination, s));
    }
    if req.medium == Medium::Images
        && req.destination != Destination::Device
        && !s.remote_vision_consent
    {
        consider(ask(
            "vision.remote",
            "page images would be sent to a remote vision model",
        ));
    }
    out
}

/// §16.4 permission levels.
fn operation_rule(op: Operation, s: &Settings) -> Decision {
    match op {
        Operation::Read => allow("16.4.read"),
        Operation::Create if s.create_needs_approval => ask(
            "16.4.create",
            "creating files needs approval in your settings",
        ),
        Operation::Create => allow("16.4.create"),
        Operation::Execute if s.execute_approved => allow("16.4.execute"),
        Operation::Execute => ask(
            "16.4.execute",
            "running code needs approval for this workflow",
        ),
        Operation::RemoteExecute => ask(
            "16.4.remote_execute",
            "running on a remote machine needs approval",
        ),
        Operation::ExternalApi => allow("16.4.external_api"), // decided per data class below
        Operation::Destructive => ask(
            "16.4.destructive",
            "delete, overwrite and publish always need approval",
        ),
    }
}

/// §16.5 data boundary.
fn boundary_rule(class: DataClass, dest: Destination, s: &Settings) -> Decision {
    use DataClass as C;
    use Destination as D;
    match (class, dest) {
        (_, D::Device) => allow("16.5.device"),
        (C::LocalOnly, _) => deny("16.5.local_only", "this data is marked local only"),
        (C::HumanOrGenetic, D::OurServer) => deny(
            "16.5.human_genetic.server",
            "human or genetic data stays on premises",
        ),
        (C::HumanOrGenetic, D::External) => deny(
            "16.5.human_genetic.external",
            "human or genetic data never goes to an external service",
        ),
        (C::PublicPaper, _) => allow("16.5.public_paper"),
        (c, D::OurServer) => {
            let needs = matches!(
                c,
                C::ConfidentialDataset | C::UnpublishedIdea | C::DocumentEmbeddings
            );
            if needs && !s.server_consent.contains(&c) {
                ask(
                    "16.5.server_consent",
                    format!("{c:?} would be sent to the ElectroPaper server"),
                )
            } else {
                allow("16.5.server")
            }
        }
        (c, D::External) => {
            if s.external_consent.contains(&c) {
                allow("16.5.external_consent")
            } else {
                ask(
                    "16.5.external_consent",
                    format!("{c:?} would be sent to an external model or service"),
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use DataClass as C;
    use Destination as D;
    use Operation as O;

    fn req(op: O, data: &[C], dest: D) -> Request {
        Request {
            operation: op,
            data: data.iter().copied().collect(),
            destination: dest,
            medium: Medium::Text,
        }
    }

    #[derive(Debug, PartialEq, Clone, Copy)]
    enum K {
        A,
        N,
        X,
    }
    fn kind(d: &Decision) -> K {
        match d {
            Decision::Allow { .. } => K::A,
            Decision::NeedsApproval { .. } => K::N,
            Decision::Deny { .. } => K::X,
        }
    }

    #[test]
    #[allow(clippy::too_many_lines)] // one row per rule reads better than helpers
    fn rule_table() {
        let none = Settings::default();
        let consent = Settings {
            external_consent: [
                C::ConfidentialDataset,
                C::UnpublishedIdea,
                C::LocalOnly,
                C::HumanOrGenetic,
            ]
            .into(),
            server_consent: [C::ConfidentialDataset].into(),
            create_needs_approval: true,
            execute_approved: true,
            remote_vision_consent: true,
        };
        // (request, settings, expected kind, expected rule)
        let rows: Vec<(Request, &Settings, K, &str)> = vec![
            (
                req(O::Read, &[C::LocalOnly], D::Device),
                &none,
                K::A,
                "16.4.read",
            ),
            (req(O::Create, &[], D::Device), &none, K::A, "16.4.create"),
            (
                req(O::Create, &[], D::Device),
                &consent,
                K::N,
                "16.4.create",
            ),
            (req(O::Execute, &[], D::Device), &none, K::N, "16.4.execute"),
            (
                req(O::Execute, &[], D::Device),
                &consent,
                K::A,
                "16.4.execute",
            ),
            (
                req(O::RemoteExecute, &[], D::External),
                &consent,
                K::N,
                "16.4.remote_execute",
            ),
            (
                req(O::Destructive, &[], D::Device),
                &consent,
                K::N,
                "16.4.destructive",
            ),
            (
                req(O::ExternalApi, &[C::PublicPaper], D::External),
                &none,
                K::A,
                "16.4.external_api",
            ),
            (
                req(O::ExternalApi, &[C::UnpublishedIdea], D::External),
                &none,
                K::N,
                "16.5.external_consent",
            ),
            (
                req(O::ExternalApi, &[C::UnpublishedIdea], D::External),
                &consent,
                K::A,
                "16.4.external_api",
            ),
            (
                req(O::ExternalApi, &[C::ExperimentResults], D::External),
                &none,
                K::N,
                "16.5.external_consent",
            ),
            (
                req(O::ExternalApi, &[C::DocumentEmbeddings], D::External),
                &none,
                K::N,
                "16.5.external_consent",
            ),
            // Consent cannot unlock the hard boundaries.
            (
                req(O::ExternalApi, &[C::LocalOnly], D::External),
                &consent,
                K::X,
                "16.5.local_only",
            ),
            (
                req(O::ExternalApi, &[C::HumanOrGenetic], D::External),
                &consent,
                K::X,
                "16.5.human_genetic.external",
            ),
            (
                req(O::ExternalApi, &[C::HumanOrGenetic], D::OurServer),
                &consent,
                K::X,
                "16.5.human_genetic.server",
            ),
            (
                req(O::ExternalApi, &[C::ConfidentialDataset], D::OurServer),
                &none,
                K::N,
                "16.5.server_consent",
            ),
            (
                req(O::ExternalApi, &[C::ConfidentialDataset], D::OurServer),
                &consent,
                K::A,
                "16.4.external_api",
            ),
            (
                req(O::ExternalApi, &[C::ExperimentResults], D::OurServer),
                &none,
                K::A,
                "16.4.external_api",
            ),
            // The strictest class wins.
            (
                req(O::ExternalApi, &[C::PublicPaper, C::LocalOnly], D::External),
                &none,
                K::X,
                "16.5.local_only",
            ),
        ];
        for (i, (r, s, k, rule)) in rows.iter().enumerate() {
            let d = decide(r, s);
            assert_eq!((kind(&d), d.rule()), (*k, *rule), "row {i}: {r:?}");
        }
    }

    #[test]
    fn remote_vision_needs_its_own_consent() {
        let mut r = req(O::ExternalApi, &[C::PublicPaper], D::External);
        r.medium = Medium::Images;
        assert_eq!(decide(&r, &Settings::default()).rule(), "vision.remote");
        let s = Settings {
            remote_vision_consent: true,
            ..Settings::default()
        };
        assert!(decide(&r, &s).is_allowed());
        r.destination = D::Device;
        assert!(decide(&r, &Settings::default()).is_allowed());
    }

    #[test]
    fn decisions_serialise_with_a_tag() {
        let j = serde_json::to_value(decide(
            &req(O::Destructive, &[], D::Device),
            &Settings::default(),
        ))
        .expect("json");
        assert_eq!(j["decision"], "needs_approval");
        assert_eq!(j["rule"], "16.4.destructive");
    }
}
