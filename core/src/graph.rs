//! The link graph: every note as a node, every resolved `[[link]]` as an
//! edge. Built from the index, so it is as fresh as the last sync.

use std::collections::{HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::index::Index;
use crate::links::{normalise_target, LinkKind};
use crate::{Result, Vault};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GraphNode {
    pub id: String,
    pub title: String,
    #[serde(rename = "type")]
    pub note_type: String,
    pub path: String,
    /// A link target no note resolves to (drawn hollow; clicking creates the note).
    pub missing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    /// First typed relation seen on the pair, if any (`depends-on:: [[X]]`).
    pub relation: Option<String>,
    pub kind: LinkKind,
    /// How many links from `from` point at `to`.
    pub count: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Graph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

impl Graph {
    /// The sub-graph within `hops` links (either direction) of `id`.
    pub fn around(&self, id: &str, hops: usize) -> Graph {
        let mut adjacent: HashMap<&str, Vec<&str>> = HashMap::new();
        for e in &self.edges {
            adjacent.entry(e.from.as_str()).or_default().push(e.to.as_str());
            adjacent.entry(e.to.as_str()).or_default().push(e.from.as_str());
        }
        let mut keep: HashSet<&str> = HashSet::new();
        let mut queue: VecDeque<(&str, usize)> = VecDeque::new();
        if self.nodes.iter().any(|n| n.id == id) {
            keep.insert(id);
            queue.push_back((id, 0));
        }
        while let Some((cur, depth)) = queue.pop_front() {
            if depth >= hops {
                continue;
            }
            for next in adjacent.get(cur).into_iter().flatten() {
                if keep.insert(next) {
                    queue.push_back((next, depth + 1));
                }
            }
        }
        Graph {
            nodes: self
                .nodes
                .iter()
                .filter(|n| keep.contains(n.id.as_str()))
                .cloned()
                .collect(),
            edges: self
                .edges
                .iter()
                .filter(|e| keep.contains(e.from.as_str()) && keep.contains(e.to.as_str()))
                .cloned()
                .collect(),
        }
    }
}

impl Index {
    /// The whole vault as a graph. Targets are resolved the same way links
    /// are followed: title, then alias, then path or file name.
    pub fn graph(&self) -> Result<Graph> {
        let mut nodes = Vec::new();
        let mut by_title: HashMap<String, String> = HashMap::new();
        let mut by_path: HashMap<String, String> = HashMap::new();
        let mut by_stem: HashMap<String, String> = HashMap::new();
        {
            let mut stmt = self
                .conn
                .prepare("SELECT id, path, title, type FROM notes ORDER BY path")?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })?;
            for row in rows {
                let (id, path, title, note_type) = row?;
                by_title.entry(title.to_lowercase()).or_insert_with(|| id.clone());
                by_path
                    .entry(path.trim_end_matches(".md").to_lowercase())
                    .or_insert_with(|| id.clone());
                by_stem.entry(normalise_target(&path)).or_insert_with(|| id.clone());
                nodes.push(GraphNode {
                    id,
                    title,
                    note_type,
                    path,
                    missing: false,
                });
            }
        }
        let mut by_alias: HashMap<String, String> = HashMap::new();
        {
            let mut stmt = self
                .conn
                .prepare("SELECT note_id, alias FROM aliases ORDER BY note_id")?;
            for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
                let (id, alias) = row?;
                by_alias.entry(alias.to_lowercase()).or_insert(id);
            }
        }
        let resolve = |target: &str, norm: &str| -> Option<String> {
            let full = target.trim().trim_end_matches(".md").to_lowercase();
            by_title
                .get(norm)
                .or_else(|| by_alias.get(norm))
                .or_else(|| by_path.get(&full))
                .or_else(|| by_stem.get(norm))
                .cloned()
        };

        let mut edges: Vec<GraphEdge> = Vec::new();
        let mut edge_at: HashMap<(String, String), usize> = HashMap::new();
        let mut missing: HashMap<String, String> = HashMap::new();
        let mut stmt = self
            .conn
            .prepare("SELECT from_id, target, target_norm, relation, kind FROM links ORDER BY from_id, position")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, String>(4)?,
            ))
        })?;
        for row in rows {
            let (from, target, norm, relation, kind) = row?;
            let to = match resolve(&target, &norm) {
                Some(id) => id,
                None => {
                    let id = format!("missing:{norm}");
                    missing.entry(id.clone()).or_insert_with(|| target.trim().to_string());
                    id
                }
            };
            if to == from {
                continue;
            }
            let kind = if kind == "embed" {
                LinkKind::Embed
            } else {
                LinkKind::Wiki
            };
            match edge_at.get(&(from.clone(), to.clone())) {
                Some(&i) => {
                    edges[i].count += 1;
                    if edges[i].relation.is_none() {
                        edges[i].relation = relation;
                    }
                }
                None => {
                    edge_at.insert((from.clone(), to.clone()), edges.len());
                    edges.push(GraphEdge {
                        from,
                        to,
                        relation,
                        kind,
                        count: 1,
                    });
                }
            }
        }
        let mut missing: Vec<(String, String)> = missing.into_iter().collect();
        missing.sort();
        for (id, title) in missing {
            nodes.push(GraphNode {
                id,
                title,
                note_type: "missing".into(),
                path: String::new(),
                missing: true,
            });
        }
        Ok(Graph { nodes, edges })
    }

    /// Unused-parameter-free wrapper kept for symmetry with `Vault::local_graph`.
    pub fn local_graph(&self, id: &str, hops: usize) -> Result<Graph> {
        Ok(self.graph()?.around(id, hops))
    }
}

impl Vault {
    /// Every note and every resolved link between them.
    pub fn graph(&self) -> Result<Graph> {
        self.index.graph()
    }

    /// The notes within `hops` links of `id` and the links among them.
    pub fn local_graph(&self, id: &str, hops: usize) -> Result<Graph> {
        self.index.local_graph(id, hops)
    }

    /// Find a note the graph marked `missing:` by its original link text.
    pub fn missing_target(id: &str) -> Option<&str> {
        id.strip_prefix("missing:")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::links::{extract_links, Link};
    use crate::note::Note;

    fn vault_with(notes: &[(&str, &str)]) -> (tempfile::TempDir, Vault) {
        let dir = tempfile::tempdir().unwrap();
        let mut v = Vault::open(dir.path()).unwrap();
        for (title, body) in notes {
            let n = v.create(title, None, None).unwrap();
            v.write(&Note {
                body: body.to_string(),
                ..n
            })
            .unwrap();
        }
        (dir, v)
    }

    #[test]
    fn graph_resolves_links_and_marks_missing_targets() {
        let (_d, v) = vault_with(&[
            (
                "Robot",
                "Needs a [[Motor]] and a [[Motor]].\ndepends-on:: [[Battery]] and [[Nowhere]]",
            ),
            ("Motor", "Links back to [[Robot]]"),
            ("Battery", "Aliases below\n"),
        ]);
        let g = v.graph().unwrap();
        let id = |t: &str| g.nodes.iter().find(|n| n.title == t).unwrap().id.clone();
        assert_eq!(g.nodes.len(), 4, "{:?}", g.nodes);
        let missing = g.nodes.iter().find(|n| n.missing).unwrap();
        assert_eq!(missing.title, "Nowhere");
        assert_eq!(missing.id, "missing:nowhere");

        let edge = |from: &str, to: &str| g.edges.iter().find(|e| e.from == from && e.to == to).cloned();
        let motor = edge(&id("Robot"), &id("Motor")).expect("robot→motor");
        assert_eq!(motor.count, 2);
        assert_eq!(motor.relation, None);
        let battery = edge(&id("Robot"), &id("Battery")).expect("robot→battery");
        assert_eq!(battery.relation.as_deref(), Some("depends-on"));
        assert!(edge(&id("Motor"), &id("Robot")).is_some());
        assert!(edge(&id("Robot"), "missing:nowhere").is_some());
        assert_eq!(g.edges.len(), 4);
        // Sanity: the extractor and the graph agree on what a typed link is.
        let links: Vec<Link> = extract_links("depends-on:: [[Battery]]");
        assert_eq!(links[0].relation.as_deref(), Some("depends-on"));
    }

    #[test]
    fn local_graph_walks_both_directions_up_to_hops() {
        let (_d, v) = vault_with(&[("A", "[[B]]"), ("B", "[[C]]"), ("C", ""), ("D", "[[A]]"), ("E", "")]);
        let g = v.graph().unwrap();
        let id = |t: &str| g.nodes.iter().find(|n| n.title == t).unwrap().id.clone();
        let one = v.local_graph(&id("A"), 1).unwrap();
        let titles = |g: &Graph| {
            let mut t: Vec<String> = g.nodes.iter().map(|n| n.title.clone()).collect();
            t.sort();
            t
        };
        assert_eq!(titles(&one), ["A", "B", "D"]);
        assert_eq!(one.edges.len(), 2);
        let two = v.local_graph(&id("A"), 2).unwrap();
        assert_eq!(titles(&two), ["A", "B", "C", "D"]);
        assert_eq!(titles(&v.local_graph("nope", 3).unwrap()), Vec::<String>::new());
    }
}
