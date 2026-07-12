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
        if r < 1e-18 {
            return;
        }
        let g = 1.0 / r;
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
    pub(super) fn solve(self) -> Result<SolveSolution, SimulationError> {
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

        for col in 0..sz {
            let mut prow = col;
            for row in (col + 1)..sz {
                if a[row][col].abs() > a[prow][col].abs() {
                    prow = row;
                }
            }
            if a[prow][col].abs() < 1e-12 {
                return Err(SimulationError::SingularMatrix);
            }
            a.swap(col, prow);
            b.swap(col, prow);

            let piv = a[col][col];
            for row in (col + 1)..sz {
                let f = a[row][col] / piv;
                if f.abs() < 1e-18 {
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
            x[i] = b[i];
            for j in (i + 1)..sz {
                x[i] -= a[i][j] * x[j];
            }
            if a[i][i].abs() < 1e-12 {
                return Err(SimulationError::SingularMatrix);
            }
            x[i] /= a[i][i];
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
    let mut constraints: HashMap<(usize, usize), f64> = HashMap::new();
    for source in vs {
        if source.pos == source.neg {
            if source.v.abs() > 1.0e-12 {
                return Err(SimulationError::VoltageSourceConflict);
            }
            continue;
        }
        let key = if source.pos < source.neg {
            (source.pos, source.neg)
        } else {
            (source.neg, source.pos)
        };
        let signed_v = if source.pos < source.neg {
            source.v
        } else {
            -source.v
        };
        if let Some(existing) = constraints.get(&key) {
            if (existing - signed_v).abs() <= 1.0e-9 {
                return Err(SimulationError::VoltageSourceLoop);
            }
            return Err(SimulationError::VoltageSourceConflict);
        } else {
            constraints.insert(key, signed_v);
        }
    }
    Ok(())
}
