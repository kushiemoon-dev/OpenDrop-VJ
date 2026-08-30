//! `ease_out_kushie`: pure approximation of CSS `cubic-bezier(0.16, 1, 0.3,
//! 1)`, the "ease-out-expo"-ish curve used for the redesign's hover/active
//! transitions (`animate_bool_with_time_and_easing`, Step 22). Solved via
//! Newton-Raphson on the bezier's x-component rather than a closed form:
//! a cubic bezier's `y` isn't an explicit function of `x`, only of the
//! shared parameter `u`, so `x(u)` must be inverted numerically first.

const X1: f32 = 0.16;
const Y1: f32 = 1.0;
const X2: f32 = 0.3;
const Y2: f32 = 1.0;

/// Cubic bezier component for control points `(0,0)`, `(p1, _)`, `(p2, _)`,
/// `(1,1)`, evaluated at parameter `u`.
fn bezier_component(u: f32, p1: f32, p2: f32) -> f32 {
    let v = 1.0 - u;
    3.0 * v * v * u * p1 + 3.0 * v * u * u * p2 + u * u * u
}

/// Derivative of [`bezier_component`] with respect to `u`, needed for the
/// Newton-Raphson step.
fn bezier_derivative(u: f32, p1: f32, p2: f32) -> f32 {
    let v = 1.0 - u;
    3.0 * v * v * p1 + 6.0 * v * u * (p2 - p1) + 3.0 * u * u * (1.0 - p2)
}

/// Approximates `cubic-bezier(0.16, 1, 0.3, 1)`: `t` is the linear animation
/// progress in `[0, 1]` (read as the curve's x-axis), the return value is
/// the eased progress (the curve's y-axis). Values outside `[0, 1]` clamp
/// to the matching endpoint rather than extrapolating.
pub fn ease_out_kushie(t: f32) -> f32 {
    if t <= 0.0 {
        return 0.0;
    }
    if t >= 1.0 {
        return 1.0;
    }

    // x(u) is monotonic for x1, x2 in [0, 1], so Newton-Raphson on it
    // converges reliably from this starting guess in a handful of steps.
    let mut u = t;
    for _ in 0..8 {
        let dx = bezier_derivative(u, X1, X2);
        if dx.abs() < 1e-6 {
            break;
        }
        u -= (bezier_component(u, X1, X2) - t) / dx;
        u = u.clamp(0.0, 1.0);
    }
    bezier_component(u, Y1, Y2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoints_are_exact() {
        assert_eq!(ease_out_kushie(0.0), 0.0);
        assert_eq!(ease_out_kushie(1.0), 1.0);
    }

    #[test]
    fn out_of_range_inputs_clamp_to_the_matching_endpoint() {
        assert_eq!(ease_out_kushie(-0.5), 0.0);
        assert_eq!(ease_out_kushie(1.5), 1.0);
    }

    #[test]
    fn monotonically_increasing_across_the_domain() {
        let samples: Vec<f32> = (0..=20).map(|i| ease_out_kushie(i as f32 / 20.0)).collect();
        for pair in samples.windows(2) {
            assert!(pair[1] > pair[0], "not increasing: {} -> {}", pair[0], pair[1]);
        }
    }

    #[test]
    fn decelerates_fast_out_of_the_gate() {
        // `cubic-bezier(0.16,1,0.3,1)` is heavily front-loaded (both y
        // control points sit at 1.0, so by the bezier convex-hull
        // property the curve never exceeds 1.0 either): worth pinning so
        // a future change to X1/X2 that flattens the curve gets caught.
        assert!(ease_out_kushie(0.1) > 0.4, "expected a fast start, got {}", ease_out_kushie(0.1));
        assert!(ease_out_kushie(0.5) > 0.9, "expected near-settled by the midpoint, got {}", ease_out_kushie(0.5));
    }

    #[test]
    fn never_exceeds_the_1_0_ceiling() {
        for i in 0..=20 {
            let y = ease_out_kushie(i as f32 / 20.0);
            assert!((0.0..=1.0).contains(&y), "y={y} out of [0,1] at t={}", i as f32 / 20.0);
        }
    }
}
