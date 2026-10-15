//! Rules that span tickets. Checks take the ticket by value and hand it back,
//! wrapped when they pass, so a passed check can't be applied to a different ticket.

use alloc::collections::{BTreeMap, BTreeSet, VecDeque};
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::iter;

use crate::fields::{Actor, Priority};
use crate::ids::TicketId;
use crate::status::{InvalidTransition, StateName, Transition};
use crate::ticket::Ticket;
use jiff::Timestamp;

/// The statuses and edges of one project, without missing dependencies,
/// self-dependencies or cycles.
#[derive(Debug)]
pub struct ValidGraph {
    nodes: BTreeMap<TicketId, Node>,
    /// Tickets left out because they, or something they depend on, have a
    /// dependency problem.
    excluded: BTreeSet<TicketId>,
}

#[derive(Debug)]
struct Node {
    priority: Priority,
    created: Timestamp,
    state: StateName,
    deps: BTreeSet<TicketId>,
}

/// A ticket that is `Todo` with every dependency `Done`.
/// Private field: [`ValidGraph::ready`] is the only way to make one.
#[derive(Debug)]
pub struct Ready(Ticket);

impl Ready {
    #[must_use]
    pub fn ticket(&self) -> &Ticket {
        &self.0
    }

    /// The only way to start a ticket.
    ///
    /// # Errors
    /// [`InvalidTransition`] if the ticket isn't `Todo`, which `ValidGraph::ready` already ruled out.
    pub fn start(self, by: Actor, now: Timestamp) -> Result<Ticket, InvalidTransition> {
        self.0.apply(Transition::Start { by }, now)
    }
}

#[derive(Clone, PartialEq, Eq, Debug, thiserror::Error)]
pub enum GraphProblem {
    #[error("{0} appears twice")]
    Duplicate(TicketId),
    #[error("{ticket} depends on {dep}, which doesn't exist")]
    Missing { ticket: TicketId, dep: TicketId },
    #[error("{0} depends on itself")]
    OnItself(TicketId),
    #[error("{ticket} depends on {dep} in another project")]
    OtherProject { ticket: TicketId, dep: TicketId },
    #[error("dependency cycle: {}", arrows(.0))]
    Cycle(Vec<TicketId>),
}

#[derive(Clone, PartialEq, Eq, Debug, thiserror::Error)]
pub enum DepError {
    #[error("a ticket can't depend on itself")]
    OnItself,
    #[error("{0} is in another project")]
    OtherProject(TicketId),
    #[error("{0} doesn't exist")]
    Missing(TicketId),
    #[error("{0} has dependency problems; run `tk lint`")]
    Broken(TicketId),
    #[error("that dependency would create a cycle: {}", arrows(.path))]
    WouldCycle { path: Vec<TicketId> },
}

#[derive(Clone, PartialEq, Eq, Debug, thiserror::Error)]
pub enum NotReady {
    #[error("only a todo ticket can start; this one is {0}")]
    NotTodo(StateName),
    #[error("ticket is waiting on {}", arrows(.0))]
    WaitingOn(Vec<TicketId>),
    #[error("ticket has dependency problems; run `tk lint`")]
    Broken,
}

/// Every ticket's node, and every problem among them.
fn analyze<'a>(
    tickets: impl IntoIterator<Item = &'a Ticket>,
) -> (BTreeMap<TicketId, Node>, Vec<GraphProblem>) {
    let mut nodes = BTreeMap::new();
    let mut problems = Vec::new();
    for t in tickets {
        let node = Node {
            priority: t.priority,
            created: t.created(),
            state: t.status().name(),
            deps: t.depends_on().clone(),
        };
        if nodes.insert(t.id().clone(), node).is_some() {
            problems.push(GraphProblem::Duplicate(t.id().clone()));
        }
    }
    for (id, node) in &nodes {
        for dep in &node.deps {
            if dep == id {
                problems.push(GraphProblem::OnItself(id.clone()));
            } else if dep.project() != id.project() {
                problems.push(GraphProblem::OtherProject {
                    ticket: id.clone(),
                    dep: dep.clone(),
                });
            } else if !nodes.contains_key(dep) {
                problems.push(GraphProblem::Missing {
                    ticket: id.clone(),
                    dep: dep.clone(),
                });
            }
        }
    }
    problems.extend(cycles(&nodes).into_iter().map(GraphProblem::Cycle));
    (nodes, problems)
}

impl GraphProblem {
    /// The tickets this problem makes unusable.
    fn tickets(&self) -> Vec<&TicketId> {
        match self {
            Self::Duplicate(id) | Self::OnItself(id) => vec![id],
            Self::Missing { ticket, .. } | Self::OtherProject { ticket, .. } => vec![ticket],
            Self::Cycle(path) => path.iter().collect(),
        }
    }
}

impl ValidGraph {
    /// The valid part of the project, so one broken ticket doesn't stop unrelated
    /// work. A ticket with a problem, and every ticket that depends on one, is
    /// left out: it can't be started or depended on until `tk lint` comes back clean.
    pub fn build<'a>(tickets: impl IntoIterator<Item = &'a Ticket>) -> (Self, Vec<GraphProblem>) {
        let (mut nodes, problems) = analyze(tickets);
        let mut excluded: BTreeSet<TicketId> = problems
            .iter()
            .flat_map(GraphProblem::tickets)
            .cloned()
            .collect();
        loop {
            let tainted: Vec<TicketId> = nodes
                .iter()
                .filter(|(id, n)| {
                    !excluded.contains(*id) && n.deps.iter().any(|d| excluded.contains(d))
                })
                .map(|(id, _)| id.clone())
                .collect();
            if tainted.is_empty() {
                break;
            }
            excluded.extend(tainted);
        }
        nodes.retain(|id, _| !excluded.contains(id));
        (Self { nodes, excluded }, problems)
    }

    /// `Todo` tickets whose dependencies are all `Done`, by (priority, created).
    pub fn ready_ids(&self) -> impl Iterator<Item = &TicketId> {
        let mut ready: Vec<(&TicketId, &Node)> = self
            .nodes
            .iter()
            .filter(|(_, n)| n.state == StateName::Todo && n.deps.iter().all(|d| self.is_done(d)))
            .collect();
        ready.sort_by_key(|&(id, n)| (n.priority, n.created, id));
        ready.into_iter().map(|(id, _)| id)
    }

    /// Wraps the ticket only if it is `Todo` and its dependencies are `Done`.
    ///
    /// # Errors
    /// The ticket back, with why it isn't ready.
    #[expect(
        clippy::result_large_err,
        reason = "the error hands the ticket back by value; Ok holds the same ticket, so boxing saves nothing"
    )]
    pub fn ready(&self, t: Ticket) -> Result<Ready, (Ticket, NotReady)> {
        if self.excluded.contains(t.id()) {
            return Err((t, NotReady::Broken));
        }
        match t.status().name() {
            StateName::Todo => {}
            state @ (StateName::Doing | StateName::Blocked | StateName::Done) => {
                return Err((t, NotReady::NotTodo(state)));
            }
        }
        let waiting_on: Vec<TicketId> = t
            .depends_on()
            .iter()
            .filter(|d| !self.is_done(d))
            .cloned()
            .collect();
        if waiting_on.is_empty() {
            Ok(Ready(t))
        } else {
            Err((t, NotReady::WaitingOn(waiting_on)))
        }
    }

    /// Adds the dependency only if it exists in this project and creates no cycle.
    ///
    /// # Errors
    /// The ticket back, unchanged, with why the dependency was refused.
    #[expect(
        clippy::result_large_err,
        reason = "the error hands the ticket back by value; Ok holds the same ticket, so boxing saves nothing"
    )]
    pub fn add_dep(&self, t: Ticket, dep: TicketId) -> Result<Ticket, (Ticket, DepError)> {
        let id = t.id();
        let problem = if &dep == id {
            Some(DepError::OnItself)
        } else if dep.project() != id.project() {
            Some(DepError::OtherProject(dep.clone()))
        } else if self.excluded.contains(&dep) {
            Some(DepError::Broken(dep.clone()))
        } else if !self.nodes.contains_key(&dep) {
            Some(DepError::Missing(dep.clone()))
        } else {
            // The new edge id -> dep closes a cycle exactly when dep already reaches id.
            self.path(&dep, id).map(|path| DepError::WouldCycle {
                path: iter::once(id.clone()).chain(path).collect(),
            })
        };
        match problem {
            Some(e) => Err((t, e)),
            None => Ok(t.with_dep(dep)),
        }
    }

    fn is_done(&self, id: &TicketId) -> bool {
        self.nodes
            .get(id)
            .is_some_and(|n| n.state == StateName::Done)
    }

    /// The shortest dependency path from `from` to `to`, both included.
    fn path(&self, from: &TicketId, to: &TicketId) -> Option<Vec<TicketId>> {
        let mut parent: BTreeMap<&TicketId, &TicketId> = BTreeMap::new();
        let mut queue = VecDeque::from([from]);
        while let Some(at) = queue.pop_front() {
            if at == to {
                let mut path = vec![at.clone()];
                let mut cur = at;
                // The graph is acyclic and `from` has no parent, so this ends.
                while let Some(&p) = parent.get(cur) {
                    path.push(p.clone());
                    cur = p;
                }
                path.reverse();
                return Some(path);
            }
            for dep in deps_of(&self.nodes, at) {
                if dep != from && !parent.contains_key(dep) {
                    parent.insert(dep, at);
                    queue.push_back(dep);
                }
            }
        }
        None
    }
}

/// Existing dependencies of `id`, other than itself (that's reported on its own).
fn deps_of<'a>(
    nodes: &'a BTreeMap<TicketId, Node>,
    id: &'a TicketId,
) -> impl Iterator<Item = &'a TicketId> {
    nodes
        .get(id)
        .into_iter()
        .flat_map(|n| n.deps.iter())
        .filter(move |&dep| dep != id && nodes.contains_key(dep))
}

/// Every cycle found by a depth-first search, each as a path whose first and last IDs are equal.
fn cycles(nodes: &BTreeMap<TicketId, Node>) -> Vec<Vec<TicketId>> {
    let mut finished: BTreeSet<&TicketId> = BTreeSet::new();
    let mut found = Vec::new();
    for root in nodes.keys() {
        if finished.contains(&root) {
            continue;
        }
        let mut path = vec![root];
        let mut stack = vec![deps_of(nodes, root)];
        while let Some(step) = stack.last_mut().map(Iterator::next) {
            match step {
                Some(dep) if finished.contains(&dep) => {}
                Some(dep) => {
                    if let Some(start) = path.iter().position(|&p| p == dep) {
                        let mut cycle: Vec<TicketId> =
                            path.iter().skip(start).map(|&p| p.clone()).collect();
                        cycle.push(dep.clone());
                        found.push(cycle);
                    } else {
                        path.push(dep);
                        stack.push(deps_of(nodes, dep));
                    }
                }
                None => {
                    stack.pop();
                    if let Some(done) = path.pop() {
                        finished.insert(done);
                    }
                }
            }
        }
    }
    found
}

fn arrows(path: &[TicketId]) -> String {
    path.iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" -> ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NewTicket, Title};

    fn ticket(id: &str, deps: &[&str]) -> Ticket {
        let spec = NewTicket::builder()
            .title(Title::try_new(String::from("t")).unwrap())
            .build();
        deps.iter().fold(
            Ticket::new(spec, id.parse().unwrap(), Timestamp::UNIX_EPOCH),
            |t, d| t.with_dep(d.parse().unwrap()),
        )
    }

    fn id(s: &str) -> TicketId {
        s.parse().unwrap()
    }

    /// Built from runtime data, so types can't cover it: every problem is reported, not just the first.
    #[test]
    fn cycle_and_missing_dependency_are_both_reported() {
        let tickets = [
            ticket("app-aaaa", &["app-bbbb"]),
            ticket("app-bbbb", &["app-aaaa"]),
            ticket("app-cccc", &["app-zzzz"]),
        ];
        let problems = ValidGraph::build(&tickets).1;
        assert_eq!(
            problems,
            vec![
                GraphProblem::Missing {
                    ticket: id("app-cccc"),
                    dep: id("app-zzzz")
                },
                GraphProblem::Cycle(vec![id("app-aaaa"), id("app-bbbb"), id("app-aaaa")]),
            ]
        );
    }

    /// A broken ticket and what depends on it are set aside; unrelated tickets stay startable.
    #[test]
    fn usable_graph_sets_aside_only_what_a_problem_touches() {
        let tickets = [
            ticket("app-aaaa", &["app-zzzz"]),
            ticket("app-bbbb", &["app-aaaa"]),
            ticket("app-cccc", &[]),
        ];
        let (graph, problems) = ValidGraph::build(&tickets);
        assert_eq!(problems.len(), 1);
        assert_eq!(graph.ready_ids().collect::<Vec<_>>(), vec![&id("app-cccc")]);
        let [_, b, c] = tickets;
        assert!(matches!(graph.ready(b), Err((_, NotReady::Broken))));
        assert!(graph.ready(c).is_ok());
    }

    #[test]
    fn add_dep_refuses_a_cycle_and_names_it() {
        let tickets = [
            ticket("app-aaaa", &["app-bbbb"]),
            ticket("app-bbbb", &["app-cccc"]),
            ticket("app-cccc", &[]),
        ];
        let (graph, _) = ValidGraph::build(&tickets);
        let [_, _, c] = tickets;
        let (_, err) = graph.add_dep(c, id("app-aaaa")).unwrap_err();
        assert_eq!(
            err,
            DepError::WouldCycle {
                path: vec![
                    id("app-cccc"),
                    id("app-aaaa"),
                    id("app-bbbb"),
                    id("app-cccc")
                ]
            }
        );
    }
}
