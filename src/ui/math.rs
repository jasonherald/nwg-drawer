use exmex::{BinOp, Express, FlatEx, FloatOpsFactory, MakeOperators, Operator};
use gtk4::prelude::*;

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
        Err(_) => return MathResult::NotMath,
    };
    // Pure arithmetic only — expressions with free variables (unknown
    // identifiers that aren't our registered constants) are treated as
    // "not math". The empty binding slice makes `exmex::Express::eval`
    // return Err for any unresolved variable, which matches the intent.
    match parsed.eval(&[]) {
        Ok(val) if val.is_nan() => MathResult::Error("undefined".to_string()),
        Ok(val) if val.is_infinite() => MathResult::Error("overflow".to_string()),
        Ok(val) => MathResult::Value(val),
        Err(_) => MathResult::NotMath,
    }
}

/// Builds an inline math result widget for the search well.
/// Returns `None` if the phrase isn't a math expression.
pub fn build_math_result(phrase: &str) -> Option<gtk4::Box> {
    let (label_text, result_str) = match eval_expression(phrase) {
        MathResult::Value(val) => {
            let r = format_result(val);
            (format!("{} = {}", phrase, r), Some(r))
        }
        MathResult::Error(msg) => (format!("{} — {}", phrase, msg), None),
        MathResult::NotMath => return None,
    };

    let vbox = gtk4::Box::new(
        gtk4::Orientation::Vertical,
        super::constants::MATH_VBOX_SPACING,
    );
    vbox.set_halign(gtk4::Align::Center);
    vbox.set_margin_top(super::constants::STATUS_AREA_VERTICAL_MARGIN);
    vbox.set_margin_bottom(super::constants::STATUS_AREA_VERTICAL_MARGIN);
    // Don't set focusable(false) — the capture-phase key controller needs
    // the container to participate in event dispatch for arrow key navigation.

    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    row.set_halign(gtk4::Align::Center);

    let label = gtk4::Label::new(Some(&label_text));
    label.add_css_class("math-result");
    label.set_halign(gtk4::Align::End);
    label.set_focusable(false);
    row.append(&label);

    if let Some(result_copy) = result_str {
        append_copy_button(&row, &vbox, result_copy);
    } else {
        vbox.append(&row);
    }

    // Capture-phase key controller on the vbox (parent) — fires before
    // GTK's focus machinery processes arrow keys on the child Copy button.
    // Same pattern as install_grid_nav on FlowBoxes (navigation.rs).
    let vbox_weak = vbox.downgrade();
    let nav_ctrl = gtk4::EventControllerKey::new();
    nav_ctrl.set_propagation_phase(gtk4::PropagationPhase::Capture);
    nav_ctrl.connect_key_pressed(move |_, keyval, _, _| {
        let Some(vbox_ref) = vbox_weak.upgrade() else {
            return gtk4::glib::Propagation::Proceed;
        };
        match keyval {
            gtk4::gdk::Key::Down | gtk4::gdk::Key::Tab => {
                if super::navigation::focus_next_sibling(&vbox_ref) {
                    gtk4::glib::Propagation::Stop
                } else {
                    gtk4::glib::Propagation::Proceed
                }
            }
            gtk4::gdk::Key::Up | gtk4::gdk::Key::ISO_Left_Tab => {
                if super::navigation::focus_prev_sibling(&vbox_ref) {
                    gtk4::glib::Propagation::Stop
                } else {
                    gtk4::glib::Propagation::Proceed
                }
            }
            _ => gtk4::glib::Propagation::Proceed,
        }
    });
    vbox.add_controller(nav_ctrl);

    // Load math CSS once (dimensions from ui/constants.rs)
    use super::constants::{
        MATH_BORDER_RADIUS, MATH_BUTTON_PADDING_H, MATH_BUTTON_PADDING_V, MATH_FONT_SIZE,
        MATH_SPACING,
    };
    static CSS_LOADED: std::sync::Once = std::sync::Once::new();
    CSS_LOADED.call_once(|| {
        let provider = gtk4::CssProvider::new();
        provider.load_from_data(&format!(
            ".math-result {{ font-size: {fs}px; font-weight: bold; margin-right: {sp}px; }} \
             .math-divider {{ margin-left: 0px; margin-right: 0px; }} \
             .math-copy {{ font-size: {fs}px; background: #5b9bd5; color: white; border-radius: {br}px; padding: {pv}px {ph}px; margin-left: {sp}px; }} \
             .math-copy:hover {{ background: #4a8bc2; }} \
             .math-copied {{ color: #5b9bd5; font-style: italic; }}",
            fs = MATH_FONT_SIZE,
            sp = MATH_SPACING,
            br = MATH_BORDER_RADIUS,
            pv = MATH_BUTTON_PADDING_V,
            ph = MATH_BUTTON_PADDING_H,
        ));
        if let Some(display) = gtk4::gdk::Display::default() {
            gtk4::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
    });

    Some(vbox)
}

/// Appends a copy button and "Copied!" label to the math result row.
/// Copies the result to clipboard via wl-copy on click, with a 2-second
/// confirmation label that resets on repeated clicks.
fn append_copy_button(row: &gtk4::Box, vbox: &gtk4::Box, result_copy: String) {
    let sep = gtk4::Separator::new(gtk4::Orientation::Vertical);
    sep.add_css_class("math-divider");
    row.append(&sep);

    let copy_btn = gtk4::Button::with_label("Copy");
    copy_btn.add_css_class("math-copy");
    copy_btn.set_focusable(true);
    copy_btn.set_halign(gtk4::Align::Start);

    let copied_label = gtk4::Label::new(Some("Copied!"));
    copied_label.add_css_class("math-copied");
    copied_label.set_visible(false);

    let copied_ref = copied_label.clone();
    let pending_timer: std::rc::Rc<std::cell::Cell<Option<gtk4::glib::SourceId>>> =
        std::rc::Rc::new(std::cell::Cell::new(None));
    let timer_ref = std::rc::Rc::clone(&pending_timer);
    copy_btn.connect_clicked(move |_| {
        let mut cmd = std::process::Command::new("wl-copy");
        cmd.arg(&result_copy);
        match cmd.spawn() {
            Ok(child) => nwg_common::launch::reap_child(child, "wl-copy".to_string()),
            Err(_) => return, // wl-copy not available — skip "Copied!" feedback
        }
        // Cancel previous hide timer so repeated clicks reset the 2s window
        if let Some(id) = timer_ref.take() {
            id.remove();
        }
        copied_ref.set_visible(true);
        let hide_ref = copied_ref.clone();
        let timer_reset = std::rc::Rc::clone(&timer_ref);
        let id = gtk4::glib::timeout_add_local_once(
            std::time::Duration::from_secs(super::constants::COPIED_LABEL_TIMEOUT_SECS),
            move || {
                hide_ref.set_visible(false);
                timer_reset.set(None);
            },
        );
        timer_ref.set(Some(id));
    });
    row.append(&copy_btn);
    vbox.append(row);
    vbox.append(&copied_label);
}

fn format_result(value: f64) -> String {
    // Snap to zero at display precision (6 decimal places) so
    // expressions like sin(pi) show "0" instead of "1.2e-16"
    if value.abs() < 0.5e-6 {
        return "0".to_string();
    }
    // Show integers without decimal point (up to i64 safe range)
    if value == value.floor() && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else if value.abs() >= 1e15 || (value != 0.0 && value.abs() < 1e-4) {
        // Scientific notation for very large or very small numbers.
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
        // 6 decimal places, trailing zeros stripped for clean display
        let formatted = format!("{:.6}", value)
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string();
        // Normalize -0 to 0 (e.g. sin(-pi) rounds to -0)
        if formatted == "-0" {
            "0".to_string()
        } else {
            formatted
        }
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
