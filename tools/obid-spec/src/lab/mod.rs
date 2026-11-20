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
//! `lanes` (Linux lanes, Codex as T3 runs it), `sphere` (the Obysphere around them), and for the
//! browser's build (https://claude.ai/artifact/TVVtw8kq77vnm81B5UfcXq) `browserapi` (its API and
//! agents' tools, with `browser_contract`), `ui` (its screens, driven) and `browsersphere` (the
//! Obysphere around it); and for Oby Artifacts, `artifacts` (the store, export and import, and the
//! turn's artifact, with `artifacts_contract`) and `artifactview` (Oby's runtime of Claude Design's
//! boards, the canvas host, the artifact tab and `oby_artifact_check`).

use anyhow::{bail, Result};
use serde_json::{json, Value};

use crate::obid::Run;

pub mod artifacts;
pub mod artifacts_contract;
pub mod artifactview;
pub mod browser;
pub mod browser_cert;
pub mod browser_contract;
pub mod browserapi;
pub mod browsersphere;
pub mod desktop;
pub mod lanes;
pub mod machine;
pub mod remote;
pub mod sphere;
pub mod ui;

/// A lab workflow's area: `workflows/lab/<feature>/` → `lab-<feature>`.
pub const AREA_PREFIX: &str = "lab-";
/// The tag every lab workflow carries.
pub const TAG: &str = "lab";

const FEATURES: [&[(&str, &str)]; 11] = [
    machine::NODES,
    remote::NODES,
    browser::NODES,
    desktop::NODES,
    lanes::NODES,
    sphere::NODES,
    browserapi::NODES,
    ui::NODES,
    browsersphere::NODES,
    artifacts::NODES,
    artifactview::NODES,
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
    } else if owns(desktop::NODES) {
        desktop::run_node(run, name, input).await
    } else if owns(lanes::NODES) {
        lanes::run_node(run, name, input).await
    } else if owns(sphere::NODES) {
        sphere::run_node(run, name, input).await
    } else if owns(browserapi::NODES) {
        browserapi::run_node(run, name, input).await
    } else if owns(ui::NODES) {
        ui::run_node(run, name, input).await
    } else if owns(browsersphere::NODES) {
        browsersphere::run_node(run, name, input).await
    } else if owns(artifacts::NODES) {
        artifacts::run_node(run, name, input).await
    } else if owns(artifactview::NODES) {
        artifactview::run_node(run, name, input).await
    } else {
        bail!("{name} isn't a lab node")
    }
}

/// What the lab made for this workflow and must take down at its end, whatever the workflow ended
/// in: the screens and the servers behind them first, then the browsers, then the machines.
pub async fn cleanup() -> Value {
    let ui = ui::cleanup().await;
    let artifactview = artifactview::cleanup().await;
    let artifacts = artifacts::cleanup().await;
    let browsersphere = browsersphere::cleanup().await;
    let browserapi = browserapi::cleanup().await;
    // Browsers before machines: one running on a lab machine is closed before its machine goes.
    json!({
        "ui": ui,
        "artifactview": artifactview,
        "artifacts": artifacts,
        "browsersphere": browsersphere,
        "browserapi": browserapi,
        "browsers": browser::cleanup().await,
        "machines": machine::cleanup().await,
    })
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
