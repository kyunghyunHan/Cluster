//! MNA system matrix: (N+M)×(N+M) stamping and Gaussian-elimination solver.

use std::collections::HashMap;

use super::errors::SimulationError;

// ── Matrix builder ────────────────────────────────────────────────────────────

pub(super) struct Mna {
    /// non-GND node count N
    pub(super) n: usize,
    /// voltage-source count M
    pub(super) m: usize,
    /// (N+M) × (N+M) system matrix
    a: Vec<Vec<f64>>,
    /// RHS vector length N+M
    z: Vec<f64>,
}

impl Mna {
    pub(super) fn new(n: usize, m: usize) -> Self {
        let sz = n + m;
        Mna {
            n,
            m,
            a: vec![vec![0.0; sz]; sz],
            z: vec![0.0; sz],
        }
    }

    /// Stamp a resistor with resistance `r` (Ω) between MNA nodes `a` and `b`.
    /// Node 0 = GND (reference); use 0 for ground-connected terminals.
    pub(super) fn stamp_r(&mut self, a: usize, b: usize, r: f64) {
        // A zero-ohm jumper uses a documented 1 nΩ floor, never an open.
        // Invalid resistance poisons the equations so solve returns an error.
        let g = if r.is_finite() && r >= 0.0 {
            1.0 / r.max(1e-9)
        } else {
            f64::NAN
        };
        if a > 0 {
            self.a[a - 1][a - 1] += g;
        }
        if b > 0 {
            self.a[b - 1][b - 1] += g;
        }
        if a > 0 && b > 0 {
            self.a[a - 1][b - 1] -= g;
            self.a[b - 1][a - 1] -= g;
        }
    }

    /// Stamp an ideal voltage source V_pos − V_neg = `v`.  `k` is the
    /// 0-based voltage-source index within this system.
    pub(super) fn stamp_vs(&mut self, k: usize, pos: usize, neg: usize, v: f64) {
        let ki = self.n + k;
        if pos > 0 {
            self.a[ki][pos - 1] += 1.0;
            self.a[pos - 1][ki] += 1.0;
        }
        if neg > 0 {
            self.a[ki][neg - 1] -= 1.0;
            self.a[neg - 1][ki] -= 1.0;
        }
        self.z[ki] += v;
    }

    /// Stamp an independent current source flowing from `neg` into `pos`.
    pub(super) fn stamp_is(&mut self, pos: usize, neg: usize, i: f64) {
        if pos > 0 {
            self.z[pos - 1] += i;
        }
        if neg > 0 {
            self.z[neg - 1] -= i;
        }
    }

    /// Stamp a current-controlled current source (CCCS).
    /// Output current = `gain` × I_k (branch current of VS index `k_vs`).
    /// Conventional output current flows INTO `pos` and OUT OF `neg`.
    pub(super) fn stamp_cccs(&mut self, pos: usize, neg: usize, k_vs: usize, gain: f64) {
        let ki = self.n + k_vs;
        if pos > 0 {
            self.a[pos - 1][ki] += gain;
        }
        if neg > 0 {
            self.a[neg - 1][ki] -= gain;
        }
    }

    /// Stamp a voltage-controlled current source (VCCS / transconductance).
    /// Output current = `gm` × (V_ctrl_p − V_ctrl_n), flows INTO `pos`.
    #[allow(dead_code)] // Reserved for controlled-source models.
    pub(super) fn stamp_vccs(
        &mut self,
        pos: usize,
        neg: usize,
        ctrl_p: usize,
        ctrl_n: usize,
        gm: f64,
    ) {
        if pos > 0 {
            if ctrl_p > 0 {
                self.a[pos - 1][ctrl_p - 1] += gm;
            }
            if ctrl_n > 0 {
                self.a[pos - 1][ctrl_n - 1] -= gm;
            }
        }
        if neg > 0 {
            if ctrl_p > 0 {
                self.a[neg - 1][ctrl_p - 1] -= gm;
            }
            if ctrl_n > 0 {
                self.a[neg - 1][ctrl_n - 1] += gm;
            }
        }
    }

    /// Solve A·x = z with Gaussian elimination + partial pivoting.
    #[allow(clippy::needless_range_loop)] // Gaussian elimination requires indexed columns.
    #[allow(dead_code)]
    pub(super) fn solve(self) -> Result<SolveSolution, SimulationError> {
        self.solve_with_cancellation(None)
    }

    #[allow(clippy::needless_range_loop)]
    pub(super) fn solve_with_cancellation(
        self,
        cancellation: Option<&crate::engine::ngspice::CancellationToken>,
    ) -> Result<SolveSolution, SimulationError> {
        let sz = self.n + self.m;
        if sz == 0 {
            return Ok(SolveSolution {
                x: vec![],
                max_kcl_residual: 0.0,
            });
        }
        let original_a = self.a.clone();
        let original_z = self.z.clone();
        let mut a = self.a;
        let mut b = self.z;
        if a.iter().flatten().chain(&b).any(|v| !v.is_finite()) {
            return Err(SimulationError::NumericalFailure);
        }
        // Equilibrate rows and columns before testing pivots: conductances
        // and ideal-source constraints use different units and magnitudes.
        for (row, rhs) in a.iter_mut().zip(&mut b) {
            let scale = row.iter().map(|v| v.abs()).fold(0.0_f64, f64::max);
            if scale == 0.0 {
                return Err(SimulationError::SingularMatrix);
            }
            for value in row {
                *value /= scale;
            }
            *rhs /= scale;
        }
        let mut column_scale = vec![0.0_f64; sz];
        for col in 0..sz {
            column_scale[col] = a.iter().map(|row| row[col].abs()).fold(0.0, f64::max);
            if column_scale[col] == 0.0 {
                return Err(SimulationError::SingularMatrix);
            }
            for row in &mut a {
                row[col] /= column_scale[col];
            }
        }
        let pivot_tolerance = f64::EPSILON * sz as f64;

        for col in 0..sz {
            if cancellation.is_some_and(|token| token.is_cancelled()) {
                return Err(SimulationError::Cancelled);
            }
            let mut prow = col;
            for row in (col + 1)..sz {
                if a[row][col].abs() > a[prow][col].abs() {
                    prow = row;
                }
            }
            if a[prow][col].abs() <= pivot_tolerance {
                return Err(SimulationError::SingularMatrix);
            }
            a.swap(col, prow);
            b.swap(col, prow);

            let piv = a[col][col];
            for row in (col + 1)..sz {
                let f = a[row][col] / piv;
                if f == 0.0 {
                    continue;
                }
                b[row] -= f * b[col];
                for j in col..sz {
                    a[row][j] -= f * a[col][j];
                }
            }
        }

        let mut x = vec![0.0; sz];
        for i in (0..sz).rev() {
            if cancellation.is_some_and(|token| token.is_cancelled()) {
                return Err(SimulationError::Cancelled);
            }
            x[i] = b[i];
            for j in (i + 1)..sz {
                x[i] -= a[i][j] * x[j];
            }
            if a[i][i].abs() <= pivot_tolerance {
                return Err(SimulationError::SingularMatrix);
            }
            x[i] /= a[i][i];
        }
        for (value, scale) in x.iter_mut().zip(column_scale) {
            *value /= scale;
        }
        if x.iter().any(|v| !v.is_finite()) {
            return Err(SimulationError::NumericalFailure);
        }
        let max_kcl_residual = original_a
            .iter()
            .take(self.n)
            .zip(&original_z)
            .map(|(row, rhs)| (row.iter().zip(&x).map(|(a, x)| a * x).sum::<f64>() - rhs).abs())
            .fold(0.0_f64, f64::max);
        Ok(SolveSolution {
            x,
            max_kcl_residual,
        })
    }
}

pub(super) struct SolveSolution {
    pub(super) x: Vec<f64>,
    pub(super) max_kcl_residual: f64,
}

// ── Voltage source validation ─────────────────────────────────────────────────

/// Detect conflicting or looping ideal voltage sources before stamping.
pub(super) fn validate_voltage_sources(
    vs: &[super::models::VsEntry],
) -> Result<(), SimulationError> {
    let mut adjacency = HashMap::<usize, Vec<(usize, f64)>>::new();
    for source in vs {
        if !source.v.is_finite() {
            return Err(SimulationError::NumericalFailure);
        }
        adjacency
            .entry(source.neg)
            .or_default()
            .push((source.pos, source.v));
        adjacency
            .entry(source.pos)
            .or_default()
            .push((source.neg, -source.v));
    }
    let mut potentials = HashMap::<usize, f64>::new();
    let mut connected_groups = 0;
    for &start in adjacency.keys() {
        if potentials.contains_key(&start) {
            continue;
        }
        connected_groups += 1;
        potentials.insert(start, 0.0);
        let mut frontier = vec![start];
        while let Some(node) = frontier.pop() {
            let voltage = potentials[&node];
            for &(next, delta) in &adjacency[&node] {
                let expected = voltage + delta;
                if let Some(&actual) = potentials.get(&next) {
                    let tolerance = 1e-9 + 1e-12 * actual.abs().max(expected.abs());
                    if (actual - expected).abs() > tolerance {
                        return Err(SimulationError::VoltageSourceConflict);
                    }
                } else {
                    potentials.insert(next, expected);
                    frontier.push(next);
                }
            }
        }
    }
    if vs.len() > adjacency.len().saturating_sub(connected_groups) {
        return Err(SimulationError::VoltageSourceLoop);
    }
    Ok(())
}

#[cfg(test)]
mod audit_tests {
    use super::*;

    #[test]
    fn audit_tiny_admittance_is_not_a_singular_matrix() {
        let mut m = Mna::new(1, 0);
        m.stamp_r(1, 0, 1e15);
        m.stamp_is(1, 0, 1e-15);
        let solved = m.solve().unwrap();
        assert!((solved.x[0] - 1.0).abs() < 1e-12);
    }

    #[test]
    fn audit_zero_resistance_is_not_silently_open() {
        let mut m = Mna::new(1, 0);
        m.stamp_r(1, 0, 0.0);
        m.stamp_is(1, 0, 1.0);
        let solved = m.solve().unwrap();
        assert!(solved.x[0].abs() <= 1e-9);
    }

    #[test]
    fn audit_nonfinite_matrix_never_returns_success() {
        let mut m = Mna::new(1, 0);
        m.stamp_r(1, 0, f64::NAN);
        assert!(m.solve().is_err());
    }

    #[test]
    fn audit_three_source_loops_are_classified() {
        let source = |pos, neg, v| super::super::models::VsEntry { id: 1, pos, neg, v };
        assert_eq!(
            validate_voltage_sources(&[source(1, 0, 5.0), source(2, 1, 3.0), source(2, 0, 9.0)]),
            Err(SimulationError::VoltageSourceConflict)
        );
        assert_eq!(
            validate_voltage_sources(&[source(1, 0, 5.0), source(2, 1, 3.0), source(2, 0, 8.0)]),
            Err(SimulationError::VoltageSourceLoop)
        );
    }
}
