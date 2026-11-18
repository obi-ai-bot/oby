//! `obid-spec coverage` (docs/contracts/obid-spec.md, OBIDSPEC-3 and 4): every route in
//! `docs/api/routes.txt` has a workflow, and every scenario a contract in the suite's scope marks
//! as obid's and implemented has an assertion naming it. `scope.json` says, per area, which
//! routes and contracts it covers, and for each contract which scenarios are obid's (to prove
//! here) and which are another surface's (the Electron app's, a script's), each with why. A
//! scenario in a contract in scope that is in neither list fails the check: nothing is left
//! unread silently.

use crate::workflow::Workflow;
use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub fn routes(root: &Path) -> Result<Vec<String>> {
    let text =
        std::fs::read_to_string(root.join("docs/api/routes.txt")).context("docs/api/routes.txt")?;
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_string)
        .collect())
}

/// The scenario ids a contract lists as acceptance scenarios (`- **SPHERE-21** …`), in order.
pub fn scenarios(text: &str) -> Vec<String> {
    let re =
        // `- **SPHERE-21** …`, `- **ACCOUNTS-1:** …`, `- **HOME-1.** …`, or a table's first cell.
        regex::Regex::new(r"^\s*(?:-\s+\*\*([A-Z][A-Z0-9]*(?:-[A-Z][A-Z0-9]*)*-[0-9]+)(?:[\s,(:.][^*]*)?\*\*|\|\s*([A-Z][A-Z0-9]*(?:-[A-Z][A-Z0-9]*)*-[0-9]+)\s*\|)").unwrap();
    let mut seen = BTreeSet::new();
    text.lines()
        .filter_map(|l| {
            re.captures(l)
                .and_then(|c| c.get(1).or(c.get(2)))
                .map(|m| m.as_str().to_string())
        })
        .filter(|id| seen.insert(id.clone()))
        .collect()
}

/// `SPHERE-4..10` → SPHERE-4 to SPHERE-10; one id as itself.
pub fn expand(item: &str) -> Vec<String> {
    if let Some((from, to)) = item.split_once("..") {
        if let Some((prefix, start)) = from.rsplit_once('-') {
            if let (Ok(a), Ok(b)) = (start.parse::<u32>(), to.parse::<u32>()) {
                return (a..=b).map(|n| format!("{prefix}-{n}")).collect();
            }
        }
    }
    vec![item.to_string()]
}

#[derive(Debug, Default)]
pub struct AreaReport {
    pub area: String,
    /// Each background trigger of the area, and its workflow files that exist.
    pub triggers: Vec<(String, Vec<String>)>,
    pub routes: Vec<(String, Vec<String>)>,
    pub scenarios_obid: Vec<(String, Vec<String>)>,
    pub elsewhere: usize,
    pub unclassified: Vec<String>,
    pub unknown_proves: Vec<String>,
}

#[derive(Debug, Default)]
pub struct Report {
    pub areas: Vec<AreaReport>,
    pub routes_total: usize,
    pub routes_covered: usize,
    pub routes_unassigned: Vec<String>,
    pub stale_workflows: Vec<String>,
    pub contracts_unread: usize,
}

impl Report {
    pub fn ok(&self) -> bool {
        self.areas.iter().all(|a| {
            a.routes.iter().all(|(_, w)| !w.is_empty())
                && a.scenarios_obid.iter().all(|(_, w)| !w.is_empty())
                && a.unclassified.is_empty()
                && a.triggers.iter().all(|(_, w)| !w.is_empty())
                && a.unknown_proves.is_empty()
        }) && self.routes_unassigned.is_empty()
            && self.stale_workflows.is_empty()
    }
}

pub fn report(
    root: &Path,
    scope: &Value,
    workflows: &[Workflow],
    only: Option<&str>,
) -> Result<Report> {
    let all_routes = routes(root)?;
    let mut by_route: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for w in workflows {
        if let Some(r) = &w.route {
            by_route.entry(r.as_str()).or_default().push(w.name.clone());
        }
    }
    let mut proven: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for w in workflows {
        for s in &w.steps {
            for p in &s.proves {
                proven
                    .entry(p.as_str())
                    .or_default()
                    .push(format!("{} › {}", w.name, s.id));
            }
        }
    }
    let areas = scope["areas"]
        .as_object()
        .context("scope.json has no areas")?;
    let mut report = Report {
        routes_total: all_routes.len(),
        routes_covered: all_routes
            .iter()
            .filter(|r| by_route.contains_key(r.as_str()))
            .count(),
        ..Default::default()
    };
    let area_of = |route: &str| -> Option<String> {
        let path = route.split_once(' ').map(|(_, p)| p).unwrap_or(route);
        areas.iter().find_map(|(name, a)| {
            a["routes"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .any(|prefix| {
                    path == prefix
                        || path.starts_with(&format!("{prefix}/"))
                        || path.starts_with(&format!("{prefix}?"))
                })
                .then(|| name.clone())
        })
    };
    let known: BTreeSet<&str> = all_routes.iter().map(String::as_str).collect();
    report.stale_workflows = by_route
        .iter()
        .filter(|(r, _)| !known.contains(**r))
        .map(|(r, w)| format!("{r} ({})", w.join(", ")))
        .collect();
    report.routes_unassigned = all_routes
        .iter()
        .filter(|r| area_of(r).is_none())
        .cloned()
        .collect();
    let mut every_scenario: BTreeSet<String> = BTreeSet::new();
    let mut read_contracts: BTreeSet<String> = BTreeSet::new();
    for (name, a) in areas {
        if only.is_some_and(|o| o != name) {
            continue;
        }
        let mut area = AreaReport {
            area: name.clone(),
            ..Default::default()
        };
        for r in all_routes
            .iter()
            .filter(|r| area_of(r).as_deref() == Some(name))
        {
            area.routes.push((
                r.clone(),
                by_route.get(r.as_str()).cloned().unwrap_or_default(),
            ));
        }
        for (trigger, t) in scope["triggers"].as_object().into_iter().flatten() {
            if t["area"] != name.as_str() {
                continue;
            }
            let files: Vec<String> = t["workflows"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .filter(|f| {
                    workflows.iter().any(|w| {
                        w.area == "triggers" && w.file.file_name().is_some_and(|n| n == *f)
                    })
                })
                .map(str::to_string)
                .collect();
            area.triggers.push((trigger.clone(), files));
        }
        for (file, c) in a["contracts"].as_object().into_iter().flatten() {
            read_contracts.insert(file.clone());
            let text = std::fs::read_to_string(root.join("docs/contracts").join(file))
                .with_context(|| format!("docs/contracts/{file}"))?;
            let listed = scenarios(&text);
            every_scenario.extend(listed.iter().cloned());
            let obid: BTreeSet<String> = c["obid"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .flat_map(expand)
                .collect();
            let elsewhere: BTreeSet<String> = c["elsewhere"]
                .as_object()
                .into_iter()
                .flatten()
                .flat_map(|(k, _)| expand(k))
                .collect();
            for id in &listed {
                if obid.contains(id) {
                    area.scenarios_obid.push((
                        id.clone(),
                        proven.get(id.as_str()).cloned().unwrap_or_default(),
                    ));
                } else if elsewhere.contains(id) {
                    area.elsewhere += 1;
                } else {
                    area.unclassified.push(format!("{file}: {id}"));
                }
            }
            for id in obid.iter().chain(&elsewhere) {
                if !listed.contains(id) {
                    area.unclassified.push(format!(
                        "{file}: {id} is in scope.json but not in the contract"
                    ));
                }
            }
        }
        report.areas.push(area);
    }
    // A step naming a scenario no contract in scope lists: a typo, or a contract not yet read.
    if only.is_none() {
        let mut unknown: Vec<String> = proven
            .iter()
            .filter(|(id, _)| !every_scenario.contains(**id))
            .map(|(id, w)| format!("{id} ({})", w.join(", ")))
            .collect();
        if let Some(first) = report.areas.first_mut() {
            first.unknown_proves.append(&mut unknown);
        }
    }
    let all_contracts = std::fs::read_dir(root.join("docs/contracts"))?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "md"))
        .count();
    report.contracts_unread = all_contracts.saturating_sub(read_contracts.len());
    Ok(report)
}

pub fn print(report: &Report) {
    println!(
        "{:<16} {:>9} {:>10} {:>18} {:>10} {:>13}",
        "area", "routes", "triggers", "obid's scenarios", "elsewhere", "unclassified"
    );
    let (mut sr, mut sp, mut so, mut st, mut stt) = (0, 0, 0, 0, 0);
    for a in &report.areas {
        let covered = a.routes.iter().filter(|(_, w)| !w.is_empty()).count();
        let proven = a
            .scenarios_obid
            .iter()
            .filter(|(_, w)| !w.is_empty())
            .count();
        sr += covered;
        sp += proven;
        so += a.scenarios_obid.len();
        let triggered = a.triggers.iter().filter(|(_, w)| !w.is_empty()).count();
        st += triggered;
        stt += a.triggers.len();
        println!(
            "{:<16} {:>9} {:>10} {:>18} {:>10} {:>13}",
            a.area,
            format!("{covered}/{}", a.routes.len()),
            format!("{triggered}/{}", a.triggers.len()),
            format!("{proven}/{}", a.scenarios_obid.len()),
            a.elsewhere,
            a.unclassified.len()
        );
    }
    println!(
        "{:<16} {:>9} {:>10} {:>18}",
        "total",
        format!("{}/{}", report.routes_covered, report.routes_total),
        format!("{st}/{stt}"),
        format!("{sp}/{so}")
    );
    let _ = sr;
    for a in &report.areas {
        for (r, w) in &a.routes {
            if w.is_empty() {
                println!("  no workflow: {r}");
            }
        }
        for (t, w) in &a.triggers {
            if w.is_empty() {
                println!("  no workflow for the trigger: {t}");
            }
        }
        for (s, w) in &a.scenarios_obid {
            if w.is_empty() {
                println!("  no assertion: {s}");
            }
        }
        for u in &a.unclassified {
            println!("  unclassified: {u}");
        }
        for u in &a.unknown_proves {
            println!("  proves a scenario no contract in scope lists: {u}");
        }
    }
    for r in &report.routes_unassigned {
        println!("  in no area of scope.json: {r}");
    }
    for w in &report.stale_workflows {
        println!("  a workflow for a route obid doesn't serve: {w}");
    }
    if report.contracts_unread > 0 {
        println!(
            "  contracts not yet in scope.json: {}",
            report.contracts_unread
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_scenarios_and_ranges() {
        let text = "## Acceptance\n\n- **SPHERE-1** A link.\n- **CONVERSATION-AGENT-41:** x\n  - **SPHERE-1** again\nnot - **SPHERE-9**\n- **HOME-1.** y\n- **PUBLISHING-27 (implemented):** z\n- **PROJECTS-1, create and list (implemented).** w\n| JOBSTATE-2 | Given |\n| x | JOBSTATE-3 |\n";
        assert_eq!(
            scenarios(text),
            [
                "SPHERE-1",
                "CONVERSATION-AGENT-41",
                "HOME-1",
                "PUBLISHING-27",
                "PROJECTS-1",
                "JOBSTATE-2"
            ]
        );
        assert_eq!(expand("SPHERE-4..6"), ["SPHERE-4", "SPHERE-5", "SPHERE-6"]);
        assert_eq!(expand("CONVERSATION-AGENT-2"), ["CONVERSATION-AGENT-2"]);
    }
}
