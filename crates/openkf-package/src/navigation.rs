//! Navigation graph of a map (`.rom`): NavigationPoint actors (PathNode,
//! PlayerStart, InventorySpot, ...) linked by `ReachSpec` objects, plus A*.

use crate::level::read_object_properties;
use crate::properties::PropValue;
use crate::reader::UnrealPackage;
use std::collections::{BinaryHeap, HashMap};

#[derive(Debug, Clone)]
pub struct NavNode {
    pub export: usize,
    pub class: String,
    pub location: [f32; 3],
}

#[derive(Debug, Clone, Copy)]
pub struct NavEdge {
    pub to: usize,
    pub distance: f32,
    pub reach_flags: u32,
}

#[derive(Debug, Default, Clone)]
pub struct NavGraph {
    pub nodes: Vec<NavNode>,
    pub edges: Vec<Vec<NavEdge>>,
}

/// UE2 reachFlags: R_WALK=1, R_FLY=2, R_SWIM=4, R_JUMP=8, R_DOOR=16, R_SPECIAL=32, R_LADDER=64,
/// R_PROSCRIBED=128, R_FORCED=256, R_PLAYERONLY=512. Walkers avoid FLY/SWIM/PROSCRIBED.
pub const R_UNUSABLE: u32 = 2 | 4 | 128;

fn location(pkg: &UnrealPackage<'_>, export: usize) -> Option<[f32; 3]> {
    match read_object_properties(pkg, export).ok()?.get("Location")? {
        PropValue::Vector(v) => Some(*v),
        _ => None,
    }
}

pub fn read_nav_graph(pkg: &UnrealPackage<'_>) -> NavGraph {
    let mut g = NavGraph::default();
    let mut index: HashMap<usize, usize> = HashMap::new();
    let mut node_of = |g: &mut NavGraph, e: usize| -> Option<usize> {
        if let Some(&i) = index.get(&e) {
            return Some(i);
        }
        let loc = location(pkg, e)?;
        g.nodes.push(NavNode { export: e, class: crate::level::export_class_name(pkg, e).to_string(), location: loc });
        g.edges.push(Vec::new());
        index.insert(e, g.nodes.len() - 1);
        Some(g.nodes.len() - 1)
    };
    for i in 0..pkg.exports.len() {
        if crate::level::export_class_name(pkg, i) != "ReachSpec" {
            continue;
        }
        let Ok(p) = read_object_properties(pkg, i) else { continue };
        let obj = |k: &str| match p.get(k) {
            Some(PropValue::Object(r)) if *r > 0 => Some((*r - 1) as usize),
            _ => None,
        };
        let int = |k: &str| match p.get(k) {
            Some(PropValue::Int(v)) => *v,
            _ => 0,
        };
        let (Some(s), Some(e)) = (obj("Start"), obj("End")) else { continue };
        if matches!(p.get("bPruned"), Some(PropValue::Bool(true))) {
            continue;
        }
        let (Some(a), Some(b)) = (node_of(&mut g, s), node_of(&mut g, e)) else { continue };
        let flags = int("reachFlags") as u32;
        let d = int("Distance") as f32;
        let dist = if d > 0.0 { d } else { dist3(g.nodes[a].location, g.nodes[b].location) };
        g.edges[a].push(NavEdge { to: b, distance: dist, reach_flags: flags });
    }
    g
}

fn dist3(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

#[derive(PartialEq)]
struct Open(f32, usize);
impl Eq for Open {}
impl PartialOrd for Open {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Open {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        o.0.partial_cmp(&self.0).unwrap_or(std::cmp::Ordering::Equal)
    }
}

impl NavGraph {
    /// Closest node to `p` (3D distance, slight preference for same height).
    pub fn nearest(&self, p: [f32; 3]) -> Option<usize> {
        (0..self.nodes.len()).min_by(|&a, &b| {
            let d = |i: usize| {
                let l = self.nodes[i].location;
                (l[0] - p[0]).hypot(l[1] - p[1]) + (l[2] - p[2]).abs() * 2.0
            };
            d(a).partial_cmp(&d(b)).unwrap()
        })
    }

    /// A* between the nodes nearest to `from` and `to`; returns node locations (then `to`).
    pub fn path(&self, from: [f32; 3], to: [f32; 3]) -> Option<Vec<[f32; 3]>> {
        let (s, t) = (self.nearest(from)?, self.nearest(to)?);
        let n = self.nodes.len();
        let mut g = vec![f32::INFINITY; n];
        let mut prev = vec![usize::MAX; n];
        let mut open = BinaryHeap::new();
        g[s] = 0.0;
        open.push(Open(dist3(self.nodes[s].location, self.nodes[t].location), s));
        while let Some(Open(_, u)) = open.pop() {
            if u == t {
                break;
            }
            for e in &self.edges[u] {
                if e.reach_flags & R_UNUSABLE != 0 {
                    continue;
                }
                let ng = g[u] + e.distance.max(1.0);
                if ng < g[e.to] {
                    g[e.to] = ng;
                    prev[e.to] = u;
                    open.push(Open(ng + dist3(self.nodes[e.to].location, self.nodes[t].location), e.to));
                }
            }
        }
        if g[t].is_infinite() {
            return None;
        }
        let mut out = vec![to];
        let mut c = t;
        while c != usize::MAX {
            out.push(self.nodes[c].location);
            c = prev[c];
        }
        out.reverse();
        Some(out)
    }
}
