use egui::Pos2;
use std::collections::HashMap;

#[derive(Default)]
pub(in crate::engine) struct ConnectivityNodes {
    pub(in crate::engine) positions: Vec<Pos2>,
    buckets: HashMap<(i32, i32), Vec<usize>>,
}

impl ConnectivityNodes {
    pub(in crate::engine) fn node_for(&mut self, position: Pos2) -> usize {
        let origin = point_bucket(position);
        let mut matching = Vec::new();
        for x in (origin.0 - 1)..=(origin.0 + 1) {
            for y in (origin.1 - 1)..=(origin.1 + 1) {
                matching.extend(
                    self.buckets
                        .get(&(x, y))
                        .into_iter()
                        .flatten()
                        .copied()
                        .filter(|&index| self.positions[index].distance(position) <= 1.0),
                );
            }
        }
        if let Some(index) = matching.into_iter().min() {
            return index;
        }
        self.positions.push(position);
        let index = self.positions.len() - 1;
        self.buckets.entry(origin).or_default().push(index);
        index
    }
}

fn point_bucket(position: Pos2) -> (i32, i32) {
    (position.x.floor() as i32, position.y.floor() as i32)
}

#[derive(Default)]
pub(in crate::engine) struct ConnectivityUnionFind {
    parent: Vec<usize>,
}

impl ConnectivityUnionFind {
    pub(in crate::engine) fn ensure(&mut self, index: usize) {
        while self.parent.len() <= index {
            self.parent.push(self.parent.len());
        }
    }

    pub(in crate::engine) fn find(&mut self, index: usize) -> usize {
        self.ensure(index);
        if self.parent[index] != index {
            self.parent[index] = self.find(self.parent[index]);
        }
        self.parent[index]
    }

    pub(in crate::engine) fn union(&mut self, left: usize, right: usize) {
        let left = self.find(left);
        let right = self.find(right);
        if left != right {
            self.parent[right] = left;
        }
    }
}
