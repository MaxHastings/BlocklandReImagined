//! Math for scripts under the names and number types authors write.
//!
//! Rhai's math functions take only floats and name a few things their own
//! way (`ceiling`, `int`, `fraction`). A whole number passed to `sqrt`, a
//! `.floor()` on a whole number, or `ceil` spelt as C, Rust and JavaScript
//! spell it threw "Function not found", not when the Add-On loaded but the
//! first time the line ran, mid-game: Rallypack's `cmd_shot` failed on every
//! shot heard from afar in v0.2.8 (`(distance * 0.6).ceil()`). TorqueScript,
//! which most ported Add-Ons come from, has one number type, so whole
//! numbers reach float math all the time.
//!
//! So every float function here also takes whole numbers (read as floats),
//! rounding a whole number returns it unchanged, and the other usual names
//! are aliases: `ceil`, `trunc`, `fract`, `pow`, `log10`, `log2` and
//! `clamp`. Rhai's own names keep working; nothing is renamed.
use rhai::{Dynamic, Engine, EvalAltResult, FLOAT, INT};

/// Float functions of one number that also take a whole number.
const UNARY: [(&str, fn(FLOAT) -> FLOAT); 20] = [
    ("sin", FLOAT::sin),
    ("cos", FLOAT::cos),
    ("tan", FLOAT::tan),
    ("sinh", FLOAT::sinh),
    ("cosh", FLOAT::cosh),
    ("tanh", FLOAT::tanh),
    ("asin", FLOAT::asin),
    ("acos", FLOAT::acos),
    ("atan", FLOAT::atan),
    ("asinh", FLOAT::asinh),
    ("acosh", FLOAT::acosh),
    ("atanh", FLOAT::atanh),
    ("to_radians", FLOAT::to_radians),
    ("to_degrees", FLOAT::to_degrees),
    ("sqrt", FLOAT::sqrt),
    ("exp", FLOAT::exp),
    ("ln", FLOAT::ln),
    // Rhai's `log` of one number is base 10.
    ("log", FLOAT::log10),
    ("log10", FLOAT::log10),
    ("log2", FLOAT::log2),
];

/// Rounding: Rhai's name, or an alias, and what it does to a float. Each is
/// a function and a property (`x.floor`), as Rhai's own are, and a whole
/// number rounds to itself.
const ROUNDING: [(&str, fn(FLOAT) -> FLOAT); 7] = [
    ("floor", FLOAT::floor),
    ("ceiling", FLOAT::ceil),
    ("ceil", FLOAT::ceil),
    ("round", FLOAT::round),
    ("int", FLOAT::trunc),
    ("trunc", FLOAT::trunc),
    ("fract", FLOAT::fract),
];

/// Float functions of two numbers: Rhai's `atan(y, x)`, `hypot(x, y)` and
/// `log(x, base)`, and `pow(x, y)` (always a float, as in C and TorqueScript;
/// `x ** y` keeps a whole number whole).
const BINARY: [(&str, fn(FLOAT, FLOAT) -> FLOAT); 4] = [
    ("atan", FLOAT::atan2),
    ("hypot", FLOAT::hypot),
    ("log", FLOAT::log),
    ("pow", FLOAT::powf),
];

pub fn register(engine: &mut Engine) {
    for (name, f) in UNARY {
        engine.register_fn(name, move |x: INT| f(x as FLOAT));
    }
    // The aliases Rhai has no float version of.
    for (name, f) in &UNARY[18..] {
        engine.register_fn(*name, *f);
    }
    for (name, f) in ROUNDING {
        if !matches!(name, "floor" | "ceiling" | "round" | "int") {
            engine.register_fn(name, f);
            engine.register_get(name, move |x: &mut FLOAT| f(*x));
        }
        let whole = move |x: INT| if name == "fract" { 0 } else { x };
        engine.register_fn(name, whole);
        engine.register_get(name, move |x: &mut INT| whole(*x));
    }
    // Rhai's `fraction` of a whole number is nothing.
    engine.register_fn("fraction", |_: INT| 0 as INT);
    engine.register_get("fraction", |_: &mut INT| 0 as INT);
    for (name, f) in BINARY {
        engine.register_fn(name, move |x: INT, y: INT| f(x as FLOAT, y as FLOAT));
        engine.register_fn(name, move |x: INT, y: FLOAT| f(x as FLOAT, y));
        engine.register_fn(name, move |x: FLOAT, y: INT| f(x, y as FLOAT));
        if name == "pow" {
            engine.register_fn(name, f);
        }
    }
    engine.register_fn("clamp", clamp);
}

/// `x` held between `low` and `high`: whole when all three are whole, a
/// float otherwise. A `low` above `high` gives `high`, rather than the
/// panic Rust's own `clamp` would.
fn clamp(x: Dynamic, low: Dynamic, high: Dynamic) -> Result<Dynamic, Box<EvalAltResult>> {
    if let (Ok(x), Ok(low), Ok(high)) = (x.as_int(), low.as_int(), high.as_int()) {
        return Ok(x.max(low).min(high).into());
    }
    let float = |v: &Dynamic| {
        v.as_float()
            .or_else(|_| v.as_int().map(|i| i as FLOAT))
            .map_err(|_| format!("clamp takes numbers, not {}", v.type_name()))
    };
    Ok(float(&x)?.max(float(&low)?).min(float(&high)?).into())
}

#[cfg(test)]
mod tests {
    use rhai::packages::{BasicMathPackage, CorePackage, LogicPackage, Package};

    fn engine() -> rhai::Engine {
        let mut engine = rhai::Engine::new_raw();
        CorePackage::new().register_into_engine(&mut engine);
        LogicPackage::new().register_into_engine(&mut engine);
        BasicMathPackage::new().register_into_engine(&mut engine);
        super::register(&mut engine);
        engine
    }
    fn float(script: &str) -> f64 {
        engine()
            .eval::<f64>(script)
            .unwrap_or_else(|e| panic!("{script}: {e}"))
    }
    fn int(script: &str) -> i64 {
        engine()
            .eval::<i64>(script)
            .unwrap_or_else(|e| panic!("{script}: {e}"))
    }

    #[test]
    fn ceil_is_ceiling_as_a_function_a_method_and_a_property() {
        assert_eq!(float("ceil(2.1)"), 3.0);
        assert_eq!(float("(95.0 * 0.6).ceil()"), 57.0);
        assert_eq!(float("let x = -2.5; x.ceil"), -2.0);
        // Rallypack's distant-shot delay, as v0.2.8 ran it.
        assert_eq!(
            int("let distance = 100.0; 10 + (distance * 0.6).ceil().to_int()"),
            70
        );
    }

    #[test]
    fn whole_numbers_reach_float_functions() {
        assert_eq!(float("sqrt(16)"), 4.0);
        assert_eq!(float("let n = 9; n.sqrt()"), 3.0);
        assert_eq!(float("sin(0)"), 0.0);
        assert_eq!(float("log(1000)"), 3.0);
        assert_eq!(float("log10(100)"), 2.0);
        assert_eq!(float("log2(8.0)"), 3.0);
        assert_eq!(float("exp(0)"), 1.0);
        assert_eq!(float("to_radians(180)"), std::f64::consts::PI);
        assert_eq!(float("hypot(3, 4)"), 5.0);
        assert_eq!(float("hypot(3, 4.0)"), 5.0);
        assert_eq!(float("atan(1, 1)"), std::f64::consts::FRAC_PI_4);
        assert_eq!(float("log(8, 2)"), 3.0);
        // Rhai's own float-only versions are unchanged.
        assert_eq!(float("sqrt(16.0)"), 4.0);
        assert_eq!(float("atan(1.0, 1.0)"), std::f64::consts::FRAC_PI_4);
    }

    #[test]
    fn rounding_a_whole_number_keeps_it_whole() {
        for f in ["floor", "ceiling", "ceil", "round", "int", "trunc"] {
            assert_eq!(int(&format!("{f}(7)")), 7, "{f}");
            assert_eq!(int(&format!("let n = -7; n.{f}()")), -7, "{f}");
            assert_eq!(int(&format!("let n = 7; n.{f}")), 7, "{f}");
        }
        assert_eq!(int("fraction(7)"), 0);
        assert_eq!(int("fract(7)"), 0);
        assert_eq!(float("trunc(-2.7)"), -2.0);
        assert_eq!(float("fract(2.25)"), 0.25);
        assert_eq!(float("floor(2.7)"), 2.0);
    }

    #[test]
    fn pow_is_a_float_and_clamp_keeps_whole_numbers_whole() {
        assert_eq!(float("pow(2, 10)"), 1024.0);
        assert_eq!(float("pow(2.0, 0.5)"), 2f64.sqrt());
        assert_eq!(float("pow(4, -1)"), 0.25);
        assert_eq!(int("clamp(15, 0, 10)"), 10);
        assert_eq!(int("clamp(-3, 0, 10)"), 0);
        assert_eq!(float("clamp(0.5, 0, 1)"), 0.5);
        assert_eq!(float("clamp(5, 0.0, 2.5)"), 2.5);
        // Bounds the wrong way round give the upper one, never a panic.
        assert_eq!(int("clamp(5, 10, 0)"), 0);
        let e = engine()
            .eval::<rhai::Dynamic>("clamp(\"a\", 0, 1)")
            .unwrap_err();
        assert!(e.to_string().contains("clamp takes numbers"), "{e}");
    }
}
