//! The route the window uses to another computer, and when it moves (docs/contracts/remote.md,
//! COMPUTERS-3): the best route that answers. It moves down on the current route's first failure,
//! and up only once a better route answered `up_after` probes in a row, so a route that flaps
//! doesn't drag the window back and forth. With none answering it says so once, naming every
//! route it tried and why. It holds no socket and no clock: whoever probes tells it what each
//! probe found.

use serde::Serialize;

use super::route::{Route, rank};

#[derive(Clone, Debug, Default)]
struct Seen {
    /// Answers in a row (0 after a failure).
    streak: u32,
    last: Option<Result<(), String>>,
}

/// A route that didn't answer, and why.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Tried {
    pub route: String,
    pub what: String,
    pub why: String,
}

/// What the window does next.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "change", rename_all = "lowercase")]
pub enum Change {
    /// It moves to `to` (from `from`, or from none).
    Moved { from: Option<String>, to: String },
    /// No route answers: Oby says so, with each route it tried.
    Lost { tried: Vec<Tried> },
}

pub struct Supervisor {
    routes: Vec<Route>,
    seen: Vec<Seen>,
    current: Option<usize>,
    up_after: u32,
    lost: bool,
}

impl Supervisor {
    pub fn new(routes: Vec<Route>, up_after: u32) -> Supervisor {
        let routes = rank(routes);
        Supervisor {
            seen: vec![Seen::default(); routes.len()],
            routes,
            current: None,
            up_after: up_after.max(1),
            lost: false,
        }
    }

    pub fn routes(&self) -> &[Route] {
        &self.routes
    }

    pub fn current(&self) -> Option<&Route> {
        self.current.map(|i| &self.routes[i])
    }

    /// Adds a route, or replaces the one with its id, in the owner's order; its health starts
    /// unknown, and the route in use stays in use.
    pub fn add(&mut self, route: Route) {
        let current = self.current().map(|r| r.id.clone());
        let mut pairs: Vec<(Route, Seen)> = self
            .routes
            .drain(..)
            .zip(self.seen.drain(..))
            .filter(|(r, _)| r.id != route.id)
            .collect();
        pairs.push((route, Seen::default()));
        pairs.sort_by_key(|(r, _)| r.kind.rank());
        (self.routes, self.seen) = pairs.into_iter().unzip();
        self.current = current.and_then(|id| self.routes.iter().position(|r| r.id == id));
    }

    /// What to probe now: the current route and every better one; all of them while none holds.
    pub fn to_probe(&self) -> Vec<Route> {
        match self.current {
            Some(i) => self.routes[..=i].to_vec(),
            None => self.routes.clone(),
        }
    }

    /// A probe's answer: `Ok` when the route reached the other computer.
    pub fn observe(&mut self, id: &str, result: Result<(), String>) {
        if let Some(i) = self.routes.iter().position(|r| r.id == id) {
            let seen = &mut self.seen[i];
            seen.streak = if result.is_ok() { seen.streak + 1 } else { 0 };
            seen.last = Some(result);
        }
    }

    /// What changed since the last decision, if anything.
    pub fn decide(&mut self) -> Option<Change> {
        let current_ok = self
            .current
            .is_some_and(|c| matches!(self.seen[c].last, Some(Ok(()))));
        let pick = (0..self.routes.len()).find(|&j| {
            let seen = &self.seen[j];
            if !matches!(seen.last, Some(Ok(()))) {
                return false;
            }
            match self.current {
                Some(c) if current_ok && j < c => seen.streak >= self.up_after,
                _ => true,
            }
        });
        if pick.is_some() && pick == self.current {
            return None;
        }
        match pick {
            Some(j) => {
                let from = self.current.map(|c| self.routes[c].id.clone());
                self.current = Some(j);
                self.lost = false;
                Some(Change::Moved {
                    from,
                    to: self.routes[j].id.clone(),
                })
            }
            None => {
                // Lost once every route was tried, and said once until one answers again.
                let all_tried = self.seen.iter().all(|s| s.last.is_some());
                self.current = None;
                if self.lost || !all_tried {
                    return None;
                }
                self.lost = true;
                Some(Change::Lost {
                    tried: self.tried(),
                })
            }
        }
    }

    /// Every route that failed its last probe, with why.
    pub fn tried(&self) -> Vec<Tried> {
        self.routes
            .iter()
            .zip(&self.seen)
            .filter_map(|(r, s)| match &s.last {
                Some(Err(why)) => Some(Tried {
                    route: r.id.clone(),
                    what: r.describe(),
                    why: why.clone(),
                }),
                _ => None,
            })
            .collect()
    }
}

/// What the window says when no route reaches `name`: every route it tried, and why.
pub fn unreachable(name: &str, tried: &[Tried]) -> String {
    let each: Vec<String> = tried
        .iter()
        .map(|t| format!("{} ({}): {}", t.route, t.what, t.why))
        .collect();
    format!(
        "Oby can't reach {name} right now. It tried {}.",
        if each.is_empty() {
            "no route".to_string()
        } else {
            each.join("; ")
        }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::pin::Pin;

    fn routes() -> Vec<Route> {
        vec![
            Route::ssh("ssh", "box", 8810).unwrap(),
            Route::direct("lan", "https://192.168.1.2:8443", Pin([1; 32]), None).unwrap(),
        ]
    }
    fn ok() -> Result<(), String> {
        Ok(())
    }
    fn cut() -> Result<(), String> {
        Err("refused".into())
    }

    #[test]
    fn computers_3_the_best_route_that_answers_is_used_and_the_next_takes_over_when_it_is_cut() {
        let mut s = Supervisor::new(routes(), 2);
        assert_eq!(
            s.to_probe().len(),
            2,
            "every route is probed while none holds"
        );
        s.observe("lan", ok());
        s.observe("ssh", ok());
        assert_eq!(
            s.decide(),
            Some(Change::Moved {
                from: None,
                to: "lan".into()
            })
        );
        assert_eq!(s.to_probe().len(), 1, "only the current one, the best");
        s.observe("lan", cut());
        assert_eq!(
            s.decide(),
            Some(Change::Moved {
                from: Some("lan".into()),
                to: "ssh".into()
            })
        );
        assert_eq!(s.decide(), None);
    }

    #[test]
    fn computers_3_the_window_moves_up_only_once_the_better_route_answered_twice() {
        let mut s = Supervisor::new(routes(), 2);
        s.observe("lan", cut());
        s.observe("ssh", ok());
        s.decide();
        assert_eq!(s.current().unwrap().id, "ssh");
        s.observe("lan", ok());
        s.observe("ssh", ok());
        assert_eq!(s.decide(), None, "one answer isn't enough");
        s.observe("lan", cut());
        s.observe("lan", ok());
        assert_eq!(s.decide(), None, "a flap starts the count again");
        s.observe("lan", ok());
        assert_eq!(
            s.decide(),
            Some(Change::Moved {
                from: Some("ssh".into()),
                to: "lan".into()
            })
        );
    }

    #[test]
    fn computers_3_when_none_answers_it_says_which_routes_it_tried_once() {
        let mut s = Supervisor::new(routes(), 2);
        s.observe("lan", cut());
        assert_eq!(s.decide(), None, "not lost before every route was tried");
        s.observe("ssh", Err("box didn't answer over SSH.".into()));
        let Some(Change::Lost { tried }) = s.decide() else {
            panic!("lost")
        };
        assert_eq!(tried.len(), 2);
        let said = unreachable("Studio", &tried);
        assert!(
            said.contains("lan (lan https://192.168.1.2:8443): refused"),
            "{said}"
        );
        assert!(said.contains("ssh (ssh box)"), "{said}");
        assert_eq!(s.decide(), None, "said once");
        s.observe("ssh", ok());
        assert!(matches!(s.decide(), Some(Change::Moved { .. })));
    }

    #[test]
    fn computers_3_a_route_added_later_keeps_the_owners_order_and_the_route_in_use() {
        let mut s = Supervisor::new(vec![Route::ssh("ssh", "box", 8810).unwrap()], 2);
        s.observe("ssh", ok());
        s.decide();
        s.add(Route::direct("lan", "https://192.168.1.2:8443", Pin([1; 32]), None).unwrap());
        assert_eq!(s.routes()[0].id, "lan");
        assert_eq!(s.current().unwrap().id, "ssh");
        assert_eq!(s.to_probe().len(), 2);
    }
}
