//! Batched evaluation of the melange-generated BJT model with parasitic
//! resistances, shared by the generated preamp and power-amp solvers.
//!
//! `fast_exp`, `safe_exp` and every expression below reproduce the generated
//! `bjt_evaluate`/`bjt_with_parasitics` operation for operation, so each
//! device's result is bit-identical to the scalar generated functions. Only
//! parameter-only quotients are computed once per sample, and independent
//! devices' inner Newton loops are interleaved.

/// Fast exp() for audio circuit simulation.
/// Input clamped to [-40, 40] (matches melange safe_exp convention).
///
/// Default: polynomial approximation (<0.0004% error, ~6x faster than libm).
/// To use hardware/libm exp, compile with: `--cfg melange_precise_exp`
#[inline(always)]
fn fast_exp(x: f64) -> f64 {
    #[cfg(melange_precise_exp)]
    {
        x.clamp(-40.0, 40.0).exp()
    }
    #[cfg(not(melange_precise_exp))]
    {
        // Range reduction + 5th-order minimax polynomial. <0.0004% max relative error.
        // No lookup tables, no libm dependency, branchless hot path.
        let x = x.clamp(-40.0, 40.0);
        const LN2_INV: f64 = std::f64::consts::LOG2_E;
        const LN2_HI: f64 = 0.6931471803691238;
        const LN2_LO: f64 = 1.9082149292705877e-10;
        const SHIFT: f64 = 6755399441055744.0; // 2^52 + 2^51
        let z = x * LN2_INV + SHIFT;
        let n_i64 = z.to_bits() as i64 - SHIFT.to_bits() as i64;
        let n = n_i64 as f64;
        let f = (x - n * LN2_HI) - n * LN2_LO;
        let p = 1.0
            + f * (1.0
                + f * (0.5
                    + f * (0.16666666666666607
                        + f * (0.04166666666665876 + f * 0.008333333333492337))));
        let pow2n = f64::from_bits(((1023 + n_i64) as u64) << 52);
        p * pow2n
    }
}

/// Safe exponential using fast_exp (clamp to prevent overflow, matches runtime [-40, 40] range)
#[inline(always)]
fn safe_exp(x: f64) -> f64 {
    fast_exp(x)
}

/// Tail of the generated `bjt_with_parasitics`: map the intrinsic Jacobian at the
/// converged internal junction voltages to external terminals.
#[inline(always)]
pub(crate) fn bjt_external_jacobian(
    ic: f64,
    ib: f64,
    jac_int: [f64; 4],
    rb: f64,
    rc: f64,
    re: f64,
) -> (f64, f64, [f64; 4]) {
    // External Jacobian: J_ext = J_device * J_F^{-1}
    // J_F = [[j11, j12], [j21, j22]] (recompute at converged point)
    let dic_dvbe = jac_int[0];
    let dic_dvbc = jac_int[1];
    let dib_dvbe = jac_int[2];
    let dib_dvbc = jac_int[3];

    let j11 = 1.0 + dib_dvbe * rb + (dic_dvbe + dib_dvbe) * re;
    let j12 = dib_dvbc * rb + (dic_dvbc + dib_dvbc) * re;
    let j21 = dib_dvbe * rb - dic_dvbe * rc;
    let j22 = 1.0 + dib_dvbc * rb - dic_dvbc * rc;

    let det = j11 * j22 - j12 * j21;
    if det.abs() < 1e-30 {
        // Fallback: return intrinsic Jacobian
        return (ic, ib, jac_int);
    }
    let inv_det = 1.0 / det;

    // J_F^{-1} = [[j22, -j12], [-j21, j11]] / det
    let fi11 = j22 * inv_det;
    let fi12 = -j12 * inv_det;
    let fi21 = -j21 * inv_det;
    let fi22 = j11 * inv_det;

    // J_ext = J_device * J_F^{-1}
    let dic_dvbe_ext = dic_dvbe * fi11 + dic_dvbc * fi21;
    let dic_dvbc_ext = dic_dvbe * fi12 + dic_dvbc * fi22;
    let dib_dvbe_ext = dib_dvbe * fi11 + dib_dvbc * fi21;
    let dib_dvbc_ext = dib_dvbe * fi12 + dib_dvbc * fi22;

    (
        ic,
        ib,
        [dic_dvbe_ext, dic_dvbc_ext, dib_dvbe_ext, dib_dvbc_ext],
    )
}

/// Arguments of the generated `bjt_with_parasitics` for one device, plus the
/// parameter-only quotients `bjt_evaluate` recomputes on every call.
#[derive(Clone, Copy)]
pub(crate) struct BjtArgs {
    vt: f64,
    sign: f64,
    use_gp: bool,
    vaf: f64,
    var: f64,
    ikf: f64,
    ikr: f64,
    ise: f64,
    isc: f64,
    rb: f64,
    rc: f64,
    re: f64,
    is: f64,
    nf_vt: f64,
    nr_vt: f64,
    ne_vt: f64,
    nc_vt: f64,
    is_bf: f64,
    is_br: f64,
    is_bf_nf_vt: f64,
    is_br_nr_vt: f64,
    ise_ne_vt: f64,
    isc_nc_vt: f64,
    is_nf_vt: f64,
    is_nr_vt: f64,
    neg_is_nr_vt: f64,
    is_nf_vt_ikf: f64,
    is_nr_vt_ikr: f64,
}

impl BjtArgs {
    #[allow(clippy::too_many_arguments)]
    #[inline(always)]
    pub(crate) fn new(
        is: f64,
        vt: f64,
        nf: f64,
        nr: f64,
        beta_f: f64,
        beta_r: f64,
        sign: f64,
        use_gp: bool,
        vaf: f64,
        var: f64,
        ikf: f64,
        ikr: f64,
        ise: f64,
        ne: f64,
        isc: f64,
        nc: f64,
        rb: f64,
        rc: f64,
        re: f64,
    ) -> Self {
        let nf_vt = nf * vt;
        let nr_vt = nr * vt;
        Self {
            vt,
            sign,
            use_gp,
            vaf,
            var,
            ikf,
            ikr,
            ise,
            isc,
            rb,
            rc,
            re,
            is,
            nf_vt,
            nr_vt,
            ne_vt: ne * vt,
            nc_vt: nc * vt,
            is_bf: is / beta_f,
            is_br: is / beta_r,
            is_bf_nf_vt: is / (beta_f * nf_vt),
            is_br_nr_vt: is / (beta_r * nr_vt),
            ise_ne_vt: ise / (ne * vt),
            isc_nc_vt: isc / (nc * vt),
            is_nf_vt: is / nf_vt,
            is_nr_vt: is / nr_vt,
            neg_is_nr_vt: -is / nr_vt,
            is_nf_vt_ikf: is / (nf_vt * ikf),
            is_nr_vt_ikr: is / (nr_vt * ikr),
        }
    }
}

/// `bjt_evaluate` with the parameter-only subexpressions taken from `a`.
/// Every remaining operation and its order is unchanged.
#[inline(always)]
fn bjt_evaluate_args(vbe: f64, vbc: f64, a: &BjtArgs) -> (f64, f64, [f64; 4]) {
    let (is, sign, ise, isc) = (a.is, a.sign, a.ise, a.isc);
    let vbe_eff = sign * vbe;
    let vbc_eff = sign * vbc;
    let nf_vt = a.nf_vt;
    let nr_vt = a.nr_vt;

    let exp_be = safe_exp(vbe_eff / nf_vt);
    let exp_bc = safe_exp(vbc_eff / nr_vt);
    let exp_be_leak = if ise > 0.0 {
        safe_exp(vbe_eff / a.ne_vt)
    } else {
        0.0
    };
    let exp_bc_leak = if isc > 0.0 {
        safe_exp(vbc_eff / a.nc_vt)
    } else {
        0.0
    };

    let i_cc = is * (exp_be - exp_bc);

    let ib_fwd = a.is_bf * (exp_be - 1.0);
    let ib_rev = a.is_br * (exp_bc - 1.0);
    let ib_leak_be = if ise > 0.0 {
        ise * (exp_be_leak - 1.0)
    } else {
        0.0
    };
    let ib_leak_bc = if isc > 0.0 {
        isc * (exp_bc_leak - 1.0)
    } else {
        0.0
    };

    let dib_fwd_dvbe = a.is_bf_nf_vt * exp_be;
    let dib_rev_dvbc = a.is_br_nr_vt * exp_bc;
    let dib_leak_dvbe = if ise > 0.0 {
        a.ise_ne_vt * exp_be_leak
    } else {
        0.0
    };
    let dib_leak_dvbc = if isc > 0.0 {
        a.isc_nc_vt * exp_bc_leak
    } else {
        0.0
    };

    if !a.use_gp {
        let ic = sign * (i_cc - a.is_br * (exp_bc - 1.0));
        let ib = sign * (ib_fwd + ib_rev + ib_leak_be + ib_leak_bc);
        let dic_dvbe = a.is_nf_vt * exp_be;
        let dic_dvbc = -a.is_nr_vt * exp_bc - a.is_br_nr_vt * exp_bc;
        return (
            ic,
            ib,
            [
                dic_dvbe,
                dic_dvbc,
                dib_fwd_dvbe + dib_leak_dvbe,
                dib_rev_dvbc + dib_leak_dvbc,
            ],
        );
    }

    let (var, vaf, ikf, ikr) = (a.var, a.vaf, a.ikf, a.ikr);
    let q1_denom = 1.0 - vbe_eff / var - vbc_eff / vaf;
    let (q1, dq1_dvbe, dq1_dvbc) = if q1_denom <= 0.0 || q1_denom.abs() < 1e-30 {
        (1.0, 0.0, 0.0)
    } else {
        let q1 = 1.0 / q1_denom;
        (q1, q1 * q1 / var, q1 * q1 / vaf)
    };

    let cbe = is * (exp_be - 1.0);
    let cbc = is * (exp_bc - 1.0);
    let q2 = cbe / ikf + cbc / ikr;
    let dq2_dvbe = a.is_nf_vt_ikf * exp_be;
    let dq2_dvbc = a.is_nr_vt_ikr * exp_bc;

    let disc = (1.0 + 4.0 * q2).max(0.0);
    let d = disc.sqrt();
    let dd_dvbe = if d > 1e-15 { 2.0 * dq2_dvbe / d } else { 0.0 };
    let dd_dvbc = if d > 1e-15 { 2.0 * dq2_dvbc / d } else { 0.0 };

    let qb = q1 * (1.0 + d) / 2.0;
    let dqb_dvbe = dq1_dvbe * (1.0 + d) / 2.0 + q1 * dd_dvbe / 2.0;
    let dqb_dvbc = dq1_dvbc * (1.0 + d) / 2.0 + q1 * dd_dvbc / 2.0;

    let ic = sign * (i_cc / qb - a.is_br * (exp_bc - 1.0));
    let ib = sign * (ib_fwd + ib_rev + ib_leak_be + ib_leak_bc);

    let dicc_dvbe = a.is_nf_vt * exp_be;
    let dicc_dvbc = a.neg_is_nr_vt * exp_bc;
    let qb2 = (qb * qb).max(1e-30);
    let quotient_dvbe = (dicc_dvbe * qb - i_cc * dqb_dvbe) / qb2;
    let quotient_dvbc = (dicc_dvbc * qb - i_cc * dqb_dvbc) / qb2;
    let d_bc_term_dvbc = a.is_br_nr_vt * exp_bc;

    (
        ic,
        ib,
        [
            quotient_dvbe,
            quotient_dvbc - d_bc_term_dvbc,
            dib_fwd_dvbe + dib_leak_dvbe,
            dib_rev_dvbc + dib_leak_dvbc,
        ],
    )
}

/// The generated `bjt_with_parasitics` for independent devices, with their inner Newton
/// loops advanced in lockstep so the CPU can overlap the latency-bound
/// iterations. Each device performs the scalar function's operations in the
/// same order; only the interleaving between devices differs.
#[inline(always)]
pub(crate) fn bjt_with_parasitics_lockstep<const K: usize>(
    ext: [[f64; 2]; K],
    args: &[BjtArgs; K],
) -> [(f64, f64, [f64; 4]); K] {
    const INNER_MAX_ITER: usize = 15;
    const INNER_TOL: f64 = 1e-10;

    let mut int = ext;
    let mut final_eval: [Option<(f64, f64, [f64; 4])>; K] = [None; K];

    for _iter in 0..INNER_MAX_ITER {
        let mut pending = false;
        for d in 0..K {
            if final_eval[d].is_some() {
                continue;
            }
            let a = &args[d];
            let [vbe_ext, vbc_ext] = ext[d];
            let [vbe_int, vbc_int] = int[d];
            let (ic, ib, jac_int) = bjt_evaluate_args(vbe_int, vbc_int, a);
            let dic_dvbe = jac_int[0];
            let dic_dvbc = jac_int[1];
            let dib_dvbe = jac_int[2];
            let dib_dvbc = jac_int[3];

            let f1 = vbe_int - vbe_ext + ib * a.rb + (ic + ib) * a.re;
            let f2 = vbc_int - vbc_ext + ib * a.rb - ic * a.rc;

            if f1.abs() < INNER_TOL && f2.abs() < INNER_TOL {
                final_eval[d] = Some((ic, ib, jac_int));
                continue;
            }

            let j11 = 1.0 + dib_dvbe * a.rb + (dic_dvbe + dib_dvbe) * a.re;
            let j12 = dib_dvbc * a.rb + (dic_dvbc + dib_dvbc) * a.re;
            let j21 = dib_dvbe * a.rb - dic_dvbe * a.rc;
            let j22 = 1.0 + dib_dvbc * a.rb - dic_dvbc * a.rc;

            let det = j11 * j22 - j12 * j21;
            if det.abs() < 1e-30 {
                final_eval[d] = Some((ic, ib, jac_int));
                continue;
            }
            let inv_det = 1.0 / det;
            let dvbe = (j22 * f1 - j12 * f2) * inv_det;
            let dvbc = (j11 * f2 - j21 * f1) * inv_det;

            let max_step = 4.0 * a.vt;
            let dvbe = dvbe.clamp(-max_step, max_step);
            let dvbc = dvbc.clamp(-max_step, max_step);

            int[d] = [vbe_int - dvbe, vbc_int - dvbc];
            pending = true;
        }
        if !pending {
            break;
        }
    }

    let mut out = [(0.0, 0.0, [0.0; 4]); K];
    for d in 0..K {
        let a = &args[d];
        let (ic, ib, jac_int) = match final_eval[d] {
            Some(eval) => eval,
            None => bjt_evaluate_args(int[d][0], int[d][1], a),
        };
        out[d] = bjt_external_jacobian(ic, ib, jac_int, a.rb, a.rc, a.re);
    }
    out
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::BjtArgs;

    /// Arguments of a generated scalar `bjt_with_parasitics` after the two
    /// external junction voltages, in declaration order.
    #[derive(Clone, Copy)]
    pub(crate) struct BjtParams {
        pub is: f64,
        pub vt: f64,
        pub nf: f64,
        pub nr: f64,
        pub beta_f: f64,
        pub beta_r: f64,
        pub sign: f64,
        pub use_gp: bool,
        pub vaf: f64,
        pub var: f64,
        pub ikf: f64,
        pub ikr: f64,
        pub ise: f64,
        pub ne: f64,
        pub isc: f64,
        pub nc: f64,
        pub rb: f64,
        pub rc: f64,
        pub re: f64,
    }

    impl BjtParams {
        pub(crate) fn args(&self) -> BjtArgs {
            BjtArgs::new(
                self.is,
                self.vt,
                self.nf,
                self.nr,
                self.beta_f,
                self.beta_r,
                self.sign,
                self.use_gp,
                self.vaf,
                self.var,
                self.ikf,
                self.ikr,
                self.ise,
                self.ne,
                self.isc,
                self.nc,
                self.rb,
                self.rc,
                self.re,
            )
        }

        /// `self` plus variants reaching the Ebers-Moll, no-leakage,
        /// no-parasitic and PNP branches.
        pub(crate) fn with_branch_variants(self) -> [BjtParams; 5] {
            [
                self,
                BjtParams {
                    use_gp: false,
                    ..self
                },
                BjtParams {
                    ise: 0.0,
                    isc: 0.0,
                    ..self
                },
                BjtParams {
                    rb: 0.0,
                    rc: 0.0,
                    re: 0.0,
                    ..self
                },
                BjtParams {
                    sign: -self.sign,
                    ..self
                },
            ]
        }
    }

    pub(crate) struct Lcg(pub u64);

    impl Lcg {
        pub(crate) fn unit(&mut self) -> f64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (self.0 >> 11) as f64 / (1u64 << 53) as f64
        }

        pub(crate) fn range(&mut self, lo: f64, hi: f64) -> f64 {
            lo + (hi - lo) * self.unit()
        }

        /// External (Vbe, Vbc) spanning cutoff, forward-active, saturation and
        /// drives far enough past the knee to exhaust the inner iterations.
        pub(crate) fn junction_voltages(&mut self) -> [f64; 2] {
            match (self.unit() * 4.0) as usize {
                0 => [self.range(0.4, 0.9), self.range(-25.0, 0.0)],
                1 => [self.range(-3.0, 1.2), self.range(-40.0, 1.2)],
                2 => [self.range(1.2, 6.0), self.range(-2.0, 6.0)],
                _ => [self.range(-1e-3, 1e-3), self.range(-1e-3, 1e-3)],
            }
        }
    }

    pub(crate) fn result_bits((ic, ib, jac): (f64, f64, [f64; 4])) -> [u64; 6] {
        [
            ic.to_bits(),
            ib.to_bits(),
            jac[0].to_bits(),
            jac[1].to_bits(),
            jac[2].to_bits(),
            jac[3].to_bits(),
        ]
    }
}
