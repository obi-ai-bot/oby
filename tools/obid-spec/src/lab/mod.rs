//! The lab (the owner's decision of 2026-11-19): every feature after wave 3 is built here first,
//! as Rust nodes and workflows under `workflows/lab/<feature>/`, and proven on real machines
//! (OrbStack's Ubuntu, Docker) with the Obysphere around it, before any of it is ported into obid
//! or the app. Once ported, a feature's workflows become its acceptance tests against obid.
//!
//! A lab workflow carries the `lab` tag and never `launch`; `obid-spec run` leaves it out unless
//! asked for (`--tag lab`, or `--only lab-<feature>`), so neither the merge queue nor a pull
//! request's run depends on a machine the lab makes. A lab node that can't run here (no OrbStack,
//! no tool) answers `skipped` with why, as `desktop.*` does (OBIDSPEC-15).
//!
//! Each feature owns one file and its node names (`lab.<feature>.<verb>`): `machine` (the
//! machines themselves), `remote` (other computers), `browser` (the browser and the desktop),
//! `lanes` (Linux lanes, Codex as T3 runs it), `sphere` (the Obysphere around them).

use anyhow::{bail, Result};
use serde_json::Value;

use crate::obid::Run;

pub mod browser;
pub mod lanes;
pub mod machine;
pub mod remote;
pub mod sphere;

/// A lab workflow's area: `workflows/lab/<feature>/` → `lab-<feature>`.
pub const AREA_PREFIX: &str = "lab-";
/// The tag every lab workflow carries.
pub const TAG: &str = "lab";

const FEATURES: [&[(&str, &str)]; 5] = [
    machine::NODES,
    remote::NODES,
    browser::NODES,
    lanes::NODES,
    sphere::NODES,
];

/// Every lab node: its name, what it does.
pub fn nodes() -> impl Iterator<Item = (&'static str, &'static str)> {
    FEATURES.into_iter().flat_map(|f| f.iter().copied())
}

pub fn is_lab(area: &str) -> bool {
    area.starts_with(AREA_PREFIX)
}

pub async fn run_node(run: &Run, name: &str, input: &Value) -> Result<Value> {
    let owns = |f: &[(&str, &str)]| f.iter().any(|(n, _)| *n == name);
    if owns(machine::NODES) {
        machine::run_node(run, name, input).await
    } else if owns(remote::NODES) {
        remote::run_node(run, name, input).await
    } else if owns(browser::NODES) {
        browser::run_node(run, name, input).await
    } else if owns(lanes::NODES) {
        lanes::run_node(run, name, input).await
    } else if owns(sphere::NODES) {
        sphere::run_node(run, name, input).await
    } else {
        bail!("{name} isn't a lab node")
    }
}

/// What the lab made for this workflow and must take down at its end (the machines), whatever
/// the workflow ended in.
pub async fn cleanup() -> Value {
    machine::cleanup().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_lab_node_is_named_lab_feature_verb_once() {
        let mut seen = std::collections::BTreeSet::new();
        for (name, _) in nodes() {
            let parts: Vec<&str> = name.split('.').collect();
            assert!(
                parts.len() == 3 && parts[0] == "lab",
                "{name} isn't lab.<feature>.<verb>"
            );
            assert!(seen.insert(name), "{name} is registered twice");
        }
    }
}
