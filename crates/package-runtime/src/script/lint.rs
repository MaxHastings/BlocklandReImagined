//! Calls a script makes to functions that exist nowhere.
//!
//! Rhai finds the function a call names only when the line runs, so a
//! misspelt or missing name (`ceil` before the game had it, a function from
//! a newer game, a typo in a rarely taken branch) loaded cleanly and failed
//! later, mid-game, often only for one weapon or one rule. Here each call in
//! a compiled script is checked by name and number of arguments against the
//! game's functions and the script's own, when the Add-On loads. Rhai cannot
//! know argument types before a call, so a name that exists with the right
//! count passes; only a name that cannot work is reported, as a warning:
//! the rest of the script still runs.
use super::Engine;
use bri_package::diag::Diagnostic;
use rhai::{AST, ASTNode, Expr, Stmt};
use std::collections::BTreeSet;

/// Every native function's name and parameter count.
pub(super) fn natives(engine: &Engine) -> BTreeSet<(String, usize)> {
    engine
        .collect_fn_metadata(
            None,
            |f| Some((f.metadata.name.to_string(), f.metadata.num_params)),
            true,
        )
        .into_iter()
        .collect()
}

/// Rhai's own keywords that are written as calls.
const KEYWORDS: [&str; 10] = [
    "print",
    "debug",
    "type_of",
    "eval",
    "Fn",
    "call",
    "curry",
    "is_shared",
    "is_def_var",
    "is_def_fn",
];

/// One call that cannot resolve: its name, arguments as written, whether it
/// was written as a method (`x.name(..)`), and its line.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Unknown {
    pub name: String,
    pub args: usize,
    pub method: bool,
    pub line: Option<usize>,
}

/// The calls in `ast` that no native function and none of its own
/// functions can answer, once per name, count and line.
pub(super) fn unknown_calls(ast: &AST, natives: &BTreeSet<(String, usize)>) -> Vec<Unknown> {
    let scripted: BTreeSet<(String, usize)> = ast
        .iter_functions()
        .map(|f| (f.name.to_string(), f.params.len()))
        .collect();
    let known = |name: &str, n: usize| {
        let key = (name.to_string(), n);
        natives.contains(&key) || scripted.contains(&key)
    };
    let mut found = BTreeSet::new();
    ast.walk(&mut |path: &[ASTNode]| {
        let (call, method, position) = match path.last() {
            Some(ASTNode::Expr(Expr::FnCall(call, position)))
            | Some(ASTNode::Stmt(Stmt::FnCall(call, position))) => (call, false, position),
            Some(ASTNode::Expr(Expr::MethodCall(call, position))) => (call, true, position),
            _ => return true,
        };
        let name = call.name.as_str();
        let identifier = name
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_');
        // (Scripts have no modules, so no call is qualified.)
        if call.is_operator_call() || !identifier || KEYWORDS.contains(&name) {
            return true;
        }
        let args = call.args.len();
        // A method's object is a native function's first parameter; a
        // script function called as a method reads it as `this`.
        let resolves = if method {
            known(name, args + 1) || scripted.contains(&(name.to_string(), args))
        } else {
            known(name, args)
        };
        if !resolves {
            found.insert(Unknown {
                name: name.to_string(),
                args,
                method,
                line: position.line(),
            });
        }
        true
    });
    found.into_iter().collect()
}

/// A warning for `unknown`, at `at` (the script's location), with what the
/// name could have meant.
pub(super) fn warning(
    unknown: &Unknown,
    at: &str,
    natives: &BTreeSet<(String, usize)>,
    ast: &AST,
) -> Diagnostic {
    let Unknown {
        name,
        args,
        method,
        line,
    } = unknown;
    let written = if *method {
        format!("`.{name}()` with {args} argument(s) after the value")
    } else {
        format!("`{name}` with {args} argument(s)")
    };
    let mut warning = Diagnostic::warning(
        "script.unknown_function",
        format!(
            "{written} is not a function of the game or this script; the line fails each time it runs"
        ),
    )
    .at(match line {
        Some(line) => format!("{at}:{line}"),
        None => at.to_string(),
    });
    let wanted = args + usize::from(*method);
    let names = natives
        .iter()
        .map(|(n, c)| (n.as_str(), *c))
        .chain(ast.iter_functions().map(|f| (f.name, f.params.len())))
        .filter(|(n, _)| !n.contains('$'));
    let mut counts = BTreeSet::new();
    let mut near = BTreeSet::new();
    for (n, c) in names {
        if n == name {
            counts.insert(c - usize::from(*method).min(c));
        } else if c == wanted && distance(n, name) <= 2 {
            near.insert(n);
        }
    }
    if !counts.is_empty() {
        let counts: Vec<String> = counts.iter().map(usize::to_string).collect();
        warning = warning.hint(format!(
            "`{name}` takes {} argument(s) here",
            counts.join(" or ")
        ));
    } else if !near.is_empty() {
        let near: Vec<String> = near.iter().take(3).map(|n| format!("`{n}`")).collect();
        warning = warning.hint(format!("did you mean {}?", near.join(" or ")));
    }
    warning
}

/// Edit distance between two short names.
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut previous = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let substitute = previous + usize::from(ca != *cb);
            previous = row[j + 1];
            row[j + 1] = substitute.min(row[j] + 1).min(previous + 1);
        }
    }
    row[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unknown(script: &str) -> Vec<(String, usize, bool, Option<usize>)> {
        let engine = super::super::sandbox();
        let ast = engine.compile(script).unwrap();
        unknown_calls(&ast, &natives(&engine))
            .into_iter()
            .map(|u| (u.name, u.args, u.method, u.line))
            .collect()
    }

    #[test]
    fn calls_to_nothing_are_found_with_their_lines() {
        assert_eq!(
            unknown("fn f(x) {\n  let a = x.sqroot();\n  frobnicate(a, 2)\n}"),
            [
                ("frobnicate".into(), 2, false, Some(3)),
                ("sqroot".into(), 0, true, Some(2)),
            ]
        );
    }

    #[test]
    fn game_rhai_and_script_functions_resolve() {
        // Game API, Rhai's library, the math aliases, script functions
        // called plainly and as methods, keywords and operators.
        let script = "fn helper(a, b) { a + b }\nfn bump() { this += 1; }\n\
            fn f(p) { let n = 1; n.bump(); let m = #{a: 1};\n\
            let d = (p * 0.6).ceil().to_int() + sqrt(4) + clamp(n, 0, 2);\n\
            let g = Fn(\"helper\"); let s = g.call(1, 2) + helper(1, 2);\n\
            if \"a\" in m && type_of(s) == \"i64\" { print(s); }\n\
            [tick(), m.keys().len(), \"x\".len(), max(1, 2.0), s.to_string()] }";
        assert_eq!(unknown(script), []);
    }

    #[test]
    fn a_warning_suggests_the_name_or_count_meant() {
        let engine = super::super::sandbox();
        let natives = natives(&engine);
        let ast = engine
            .compile("fn f(x) { x.ceilng() + helper(1) }\nfn helper(a, b) { a }")
            .unwrap();
        let found = unknown_calls(&ast, &natives);
        let hints: Vec<_> = found
            .iter()
            .map(|u| warning(u, "probe/main.rhai", &natives, &ast))
            .map(|d| (d.code, d.location.unwrap(), d.hint.unwrap_or_default()))
            .collect();
        assert_eq!(hints[0].0, "script.unknown_function");
        assert_eq!(hints[0].1, "probe/main.rhai:1");
        assert!(hints.iter().any(|h| h.2.contains("`ceiling`")), "{hints:?}");
        assert!(
            hints
                .iter()
                .any(|h| h.2 == "`helper` takes 2 argument(s) here"),
            "{hints:?}"
        );
    }
}
