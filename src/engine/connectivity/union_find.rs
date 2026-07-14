use egui::Pos2;

#[derive(Default)]
pub(in crate::engine) struct ConnectivityNodes {
    pub(in crate::engine) positions: Vec<Pos2>,
}

impl ConnectivityNodes {
    pub(in crate::engine) fn node_for(&mut self, position: Pos2) -> usize {
        if let Some(index) = self
            .positions
            .iter()
            .position(|existing| existing.distance(position) <= 1.0)
        {
            return index;
        }
        self.positions.push(position);
        self.positions.len() - 1
    }
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
