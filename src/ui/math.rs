//! Pure inline math evaluator for the search bar.
//!
//! `eval_expression` parses the search phrase via `exmex`, returning
//! [`MathResult`]. `format_result` renders an `f64` for display in the
//! well — handles integer / decimal / scientific notation, FP-noise
//! snap-to-zero, and -0 normalization.
//!
//! The widget that surfaces this in GTK lives in [`super::math_widget`];
//! this module has zero GTK deps and is fully unit-testable.

use exmex::{BinOp, Express, FlatEx, FloatOpsFactory, MakeOperators, Operator};

/// Result of attempting to evaluate a math expression.
#[derive(Debug)]
pub enum MathResult {
    /// Successfully evaluated to a numeric result.
    Value(f64),
    /// Evaluated but produced a runtime error (e.g. division by zero, overflow).
    /// Parse failures go to `NotMath` — incomplete expressions while typing are not shown.
    Error(String),
    /// Not a math expression — just a search query.
    NotMath,
}

/// Operator factory extending exmex's default float operators with the
/// user-facing behaviors meval provided so the migration (#64) is
/// transparent to people typing math in the search bar:
///
/// - `pi` as an alias for `π` — exmex ships `π` only, but humans type `pi`.
/// - `%` as a binary modulo operator — exmex documents `%` as a custom
///   operator example but doesn't register it by default.
/// - `log` redefined to base-10 — exmex's default `log` is the natural
///   logarithm (same as `ln`), whereas calculators and meval treat `log`
///   as log-base-10. `ln` stays available for natural log.
///
/// `%` priority matches exmex's `/` (prio 3), which is higher than `*`
/// (prio 2), so `10 % 3 * 2` evaluates as `(10 % 3) * 2 = 2`. Chained
/// with `/` at the same priority, exmex evaluates left-to-right:
/// `19 % 5 / 2 = (19 % 5) / 2 = 2`.
#[derive(Clone, Debug)]
struct DrawerOpsFactory;
impl MakeOperators<f64> for DrawerOpsFactory {
    fn make<'a>() -> Vec<Operator<'a, f64>> {
        let mut ops: Vec<Operator<'a, f64>> = FloatOpsFactory::<f64>::make()
            .into_iter()
            .filter(|op| op.repr() != "log")
            .collect();
        ops.push(Operator::make_unary("log", |a| a.log10()));
        ops.push(Operator::make_constant("pi", std::f64::consts::PI));
        ops.push(Operator::make_bin(
            "%",
            BinOp {
                apply: |a, b| a % b,
                prio: 3,
                is_commutative: false,
            },
        ));
        ops
    }
}

/// Evaluates a math expression using the exmex crate.
/// Supports: +, -, *, /, ^, %, parentheses, decimals,
/// functions (sin, cos, tan, sqrt, abs, ln, log, floor, ceil, signum, cbrt, tanh, ...),
/// and constants (pi, π, e).
///
/// Migrated from meval in #64 to eliminate the nom 1.2.4
/// future-incompat warning. exmex returns a single `ExError` for both
/// parse and runtime failures; we squash all Err results to `NotMath`
/// so partially-typed expressions don't show a red error inline. NaN
/// and infinity are mapped to user-facing error messages identically
/// to the old behavior.
pub fn eval_expression(expr: &str) -> MathResult {
    let trimmed = expr.trim();
    if trimmed.is_empty() {
        return MathResult::NotMath;
    }
    let parsed = match FlatEx::<f64, DrawerOpsFactory>::parse(trimmed) {
        Ok(p) => p,
        Err(e) => {
            // `trace` so developers can flip on parser-failure visibility
            // (`RUST_LOG=nwg_drawer=trace`) without the user seeing a
            // log line every time they type a non-math search.
            log::trace!("math: parse rejected '{}': {}", trimmed, e);
            return MathResult::NotMath;
        }
    };
    // Pure arithmetic only — expressions with free variables (unknown
    // identifiers that aren't our registered constants) are treated as
    // "not math". The empty binding slice makes `exmex::Express::eval`
    // return Err for any unresolved variable, which matches the intent.
    match parsed.eval(&[]) {
        Ok(val) if val.is_nan() => MathResult::Error("undefined".to_string()),
        Ok(val) if val.is_infinite() => MathResult::Error("overflow".to_string()),
        Ok(val) => MathResult::Value(val),
        Err(e) => {
            log::trace!("math: eval rejected '{}': {}", trimmed, e);
            MathResult::NotMath
        }
    }
}

/// Formats an `f64` for display in the math result row. Handles four
/// branches: snap-to-zero for FP noise, integer rendering for whole
/// values inside the i64-safe range, scientific notation for very
/// large or very small magnitudes, and trimmed decimal otherwise.
pub(super) fn format_result(value: f64) -> String {
    // Snap to zero at display precision (6 decimal places) so
    // expressions like sin(pi) show "0" instead of "1.2e-16"
    if value.abs() < 0.5e-6 {
        return "0".to_string();
    }
    // Show integers without decimal point — but only inside the
    // i64-safe range. `1e15` is below `i64::MAX` (~9.22e18) by enough
    // headroom that any whole `f64` under 1e15 round-trips through
    // `as i64` without saturation, while still covering "practical"
    // calculator results comfortably. Anything bigger gets scientific
    // notation rather than risking a saturated integer cast.
    if value == value.floor() && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else if value.abs() >= 1e15 || (value != 0.0 && value.abs() < 1e-4) {
        // Scientific notation for very large or very small numbers.
        // The `1e-4` lower bound is the breakpoint where `{:.6}` runs
        // out of significant digits — `0.00001` would render as
        // `0.000010` and lose precision on further trimming, so we
        // hand small magnitudes off to scientific instead. Above
        // `1e-4`, `{:.6}` keeps four+ significant figures.
        // Only trim zeros from the mantissa, not the exponent.
        let scientific = format!("{:.6e}", value);
        if let Some((mantissa, exponent)) = scientific.split_once('e') {
            format!(
                "{}e{}",
                mantissa.trim_end_matches('0').trim_end_matches('.'),
                exponent
            )
        } else {
            scientific
        }
    } else {
        // 6 decimal places, trailing zeros stripped for clean display.
        // Format once, mutate the buffer in place via `pop()` — no
        // second allocation for the trimmed copy.
        let mut raw = format!("{:.6}", value);
        while raw.ends_with('0') {
            raw.pop();
        }
        if raw.ends_with('.') {
            raw.pop();
        }
        // Normalize -0 to 0 (e.g. sin(-pi) rounds to -0)
        if raw == "-0" {
            raw.clear();
            raw.push('0');
        }
        raw
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval_val(expr: &str) -> f64 {
        match eval_expression(expr) {
            MathResult::Value(v) => v,
            _ => panic!("expected Value for '{}'", expr),
        }
    }

    #[test]
    fn basic_arithmetic() {
        assert_eq!(eval_val("2+2"), 4.0);
        assert_eq!(eval_val("10 - 3"), 7.0);
        assert_eq!(eval_val("6 * 7"), 42.0);
        assert_eq!(eval_val("10 / 4"), 2.5);
        assert_eq!(eval_val("10 % 3"), 1.0);
    }

    #[test]
    fn operator_precedence() {
        assert_eq!(eval_val("2 + 3 * 4"), 14.0);
        assert_eq!(eval_val("(2 + 3) * 4"), 20.0);
    }

    #[test]
    fn negative_numbers() {
        assert_eq!(eval_val("-5"), -5.0);
        assert_eq!(eval_val("3 + -2"), 1.0);
    }

    #[test]
    fn division_by_zero() {
        assert!(matches!(eval_expression("1/0"), MathResult::Error(_)));
    }

    #[test]
    fn not_math() {
        assert!(matches!(eval_expression("firefox"), MathResult::NotMath));
        assert!(matches!(eval_expression(""), MathResult::NotMath));
        assert!(matches!(eval_expression("   "), MathResult::NotMath));
        // :command prefix is not math — handled by listeners.rs
        assert!(matches!(eval_expression(":firefox"), MathResult::NotMath));
    }

    #[test]
    fn incomplete_expression_is_not_math() {
        // Incomplete expressions while typing should not show inline errors
        assert!(matches!(eval_expression("2+"), MathResult::NotMath));
        assert!(matches!(eval_expression("(3*"), MathResult::NotMath));
        assert!(matches!(eval_expression("sqrt("), MathResult::NotMath));
    }

    #[test]
    fn overflow_not_div_by_zero() {
        // 2^1024 overflows to infinity — should say "overflow", not "division by zero"
        match eval_expression("2^1024") {
            MathResult::Error(msg) => assert_eq!(msg, "overflow"),
            other => panic!("expected Error(overflow), got {:?}", other),
        }
    }

    #[test]
    #[allow(clippy::approx_constant)]
    fn decimals() {
        assert!((eval_val("3.14 * 2") - 6.28).abs() < 1e-10);
    }

    #[test]
    fn nested_parens() {
        assert_eq!(eval_val("(((1 + 2)))"), 3.0);
    }

    #[test]
    fn power_operator() {
        assert_eq!(eval_val("2^10"), 1024.0);
    }

    #[test]
    fn builtin_functions() {
        assert_eq!(eval_val("sqrt(16)"), 4.0);
        assert_eq!(eval_val("abs(-5)"), 5.0);
    }

    #[test]
    fn builtin_constants() {
        assert!((eval_val("pi") - std::f64::consts::PI).abs() < 1e-10);
        assert!((eval_val("e") - std::f64::consts::E).abs() < 1e-10);
    }

    #[test]
    fn format_integer() {
        assert_eq!(format_result(42.0), "42");
    }

    #[test]
    #[allow(clippy::approx_constant)]
    fn format_decimal() {
        assert_eq!(format_result(3.14), "3.14");
    }

    #[test]
    fn undefined_nan() {
        // sqrt(-1) produces NaN — should show "undefined"
        match eval_expression("sqrt(-1)") {
            MathResult::Error(msg) => assert_eq!(msg, "undefined"),
            _ => panic!("expected Error(undefined)"),
        }
    }

    #[test]
    fn tiny_values_snap_to_zero() {
        // Floating-point noise like sin(pi) ≈ 1.2e-16 should display as "0"
        assert_eq!(format_result(1.2e-16), "0");
        assert_eq!(format_result(-1.2e-16), "0");
    }

    #[test]
    fn negative_zero_normalized() {
        // -0.0 displays as "0" via integer branch
        assert_eq!(format_result(-0.0), "0");
        // Very small negative (like sin(-pi)) uses scientific notation, not "-0"
        assert_ne!(format_result(-1.2e-16), "-0");
    }

    #[test]
    fn format_large_number_uses_scientific() {
        let result = format_result(2.0f64.powi(1023));
        assert!(
            result.contains('e'),
            "expected scientific notation, got: {}",
            result
        );
    }

    #[test]
    fn format_tiny_number_uses_scientific() {
        let result = format_result(0.00001);
        assert!(
            result.contains('e'),
            "expected scientific notation, got: {}",
            result
        );
    }

    #[test]
    fn format_scientific_preserves_exponent() {
        // Exponents ending in 0 must not be corrupted by mantissa trimming
        let result = format_result(1e20);
        assert!(result.ends_with("e20"), "exponent corrupted: {}", result);
        // Test negative exponent with trailing zero: 1e-5 < 1e-4, above zero-snap
        let result = format_result(1e-5);
        assert!(result.contains("e-5"), "exponent corrupted: {}", result);
    }

    // ─── Regression tests for the meval → exmex migration (#64) ──────────
    //
    // These pin down behaviors that exmex doesn't provide out of the box
    // but that meval did, so our custom `DrawerOpsFactory` must continue
    // to cover them. If someone ever trims the factory these will fail
    // loudly before users see the regression.

    #[test]
    fn pi_and_pi_unicode_both_resolve() {
        // meval accepted `pi`; exmex ships only the Unicode `π` by default.
        // Our factory adds `pi` as an alias, so both must work and agree.
        let via_ascii = eval_val("pi");
        let via_unicode = eval_val("π");
        assert!((via_ascii - std::f64::consts::PI).abs() < 1e-10);
        assert!((via_unicode - std::f64::consts::PI).abs() < 1e-10);
        assert_eq!(via_ascii, via_unicode);
    }

    #[test]
    fn modulo_in_basic_and_compound_expressions() {
        // exmex doesn't register `%` by default. Our factory adds it at
        // priority 3, matching `/` and above `*` (prio 2).
        assert_eq!(eval_val("10 % 3"), 1.0);
        assert_eq!(eval_val("17 % 5"), 2.0);
        // `%` (prio 3) binds tighter than `*` (prio 2) → `(10 % 3) * 2`
        assert_eq!(eval_val("10 % 3 * 2"), 2.0);
        // Same-priority chaining with `/` → left-to-right: `(19 % 5) / 2`
        assert_eq!(eval_val("19 % 5 / 2"), 2.0);
        // `%` after `+` → `+` should happen second since `+` is lower prio
        assert_eq!(eval_val("7 + 10 % 3"), 8.0);
    }

    #[test]
    fn unbound_identifier_is_not_math() {
        // exmex parses app names as free variables — we must reject them
        // so typing "firefox" in the drawer doesn't show a math result row.
        assert!(matches!(eval_expression("xyz"), MathResult::NotMath));
        assert!(matches!(eval_expression("foo + 1"), MathResult::NotMath));
    }

    #[test]
    fn whitespace_trimming() {
        // Leading/trailing/internal whitespace must all parse the same as
        // the bare expression — meval was lenient here and users rely on it.
        assert_eq!(eval_val("  2 + 2  "), 4.0);
        assert_eq!(eval_val("2+2"), 4.0);
        assert_eq!(eval_val("2 + 2"), 4.0);
    }

    #[test]
    fn sin_pi_is_effectively_zero() {
        // Classic FP smoke test: sin(pi) is ~1.2e-16, not exactly 0.
        // format_result snaps that to "0" for display — verify the raw
        // eval is at least tiny so the display path works.
        let val = eval_val("sin(pi)");
        assert!(val.abs() < 1e-10, "sin(pi) = {} should be ~0", val);
        assert_eq!(format_result(val), "0");
    }

    #[test]
    fn common_math_functions_available() {
        // Round-up of functions the issue listed as non-negotiable.
        assert!((eval_val("cos(0)") - 1.0).abs() < 1e-10);
        assert!((eval_val("tan(0)") - 0.0).abs() < 1e-10);
        assert!((eval_val("ln(e)") - 1.0).abs() < 1e-10);
        assert!((eval_val("log(100)") - 2.0).abs() < 1e-10);
    }
}
