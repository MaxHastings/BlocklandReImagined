//! Literal effect declarations and naming overrides, never a script interpreter.
use crate::catalog::{Token, lex};
use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;
use std::collections::BTreeMap;
type IncludeLoader<'a> = dyn FnMut(&str, &str) -> Result<(String, String)> + 'a;

#[derive(Clone, Debug, Serialize)]
pub struct Declaration {
    pub class: String,
    pub name: String,
    pub source: String,
    pub fields: BTreeMap<String, String>,
}
#[derive(Default)]
pub struct Declarations {
    pub entries: Vec<Declaration>,
    pub diagnostics: Vec<String>,
}
fn control(t: &Token) -> bool {
    ["if", "for", "while", "switch", "switch$"]
        .iter()
        .any(|s| t.atom(s))
}
fn balanced(t: &[Token], i: &mut usize, open: char, close: char) -> Result<()> {
    symbol(t, i, open)?;
    let mut depth = 1;
    while depth > 0 {
        match t.get(*i).context("Truncated control-flow block")? {
            Token::Symbol(c) if *c == open => depth += 1,
            Token::Symbol(c) if *c == close => depth -= 1,
            _ => {}
        }
        *i += 1;
    }
    Ok(())
}
fn skip_statement(t: &[Token], i: &mut usize, nesting: usize) -> Result<()> {
    ensure!(nesting < 64, "Control flow too deeply nested");
    if t.get(*i).is_some_and(control) {
        let conditional = t[*i].atom("if");
        *i += 1;
        balanced(t, i, '(', ')')?;
        skip_statement(t, i, nesting + 1)?;
        if conditional && t.get(*i).is_some_and(|t| t.atom("else")) {
            *i += 1;
            skip_statement(t, i, nesting + 1)?;
        }
    } else if t.get(*i) == Some(&Token::Symbol('{')) {
        balanced(t, i, '{', '}')?;
    } else {
        loop {
            match t.get(*i).context("Truncated statement")? {
                Token::Symbol(';') => {
                    *i += 1;
                    break;
                }
                Token::Symbol('{') => {
                    balanced(t, i, '{', '}')?;
                }
                _ => *i += 1,
            }
        }
    }
    Ok(())
}
fn symbol(t: &[Token], i: &mut usize, c: char) -> Result<()> {
    ensure!(
        t.get(*i) == Some(&Token::Symbol(c)),
        "Expected {c} at token {i}: {:?}",
        &t[i.saturating_sub(5)..(*i + 4).min(t.len())]
    );
    *i += 1;
    Ok(())
}
fn literal(t: &[Token], i: &mut usize) -> Result<String> {
    let value = match t.get(*i) {
        Some(Token::String(s)) => {
            *i += 1;
            s.clone()
        }
        Some(Token::Atom(s)) if !s.contains(['$', '%']) => {
            *i += 1;
            s.clone()
        }
        Some(Token::Symbol('-' | '+')) => {
            let sign = t[*i].clone();
            *i += 1;
            let number = t.get(*i).context("Missing signed number")?.literal()?;
            let n: f32 = number.parse()?;
            ensure!(n.is_finite(), "Nonfinite literal");
            *i += 1;
            if sign == Token::Symbol('-') {
                format!("-{number}")
            } else {
                number
            }
        }
        _ => bail!("Nonliteral effect field at token {i}"),
    };
    Ok(value)
}
impl Declarations {
    /// Inputs must already be expanded in literal include order by the offline tool.
    pub fn read(&mut self, source: &str, origin: &str) -> Result<()> {
        self.read_classes(
            source,
            origin,
            &[
                "fxLightData",
                "ParticleData",
                "ParticleEmitterData",
                "ParticleEmitterNodeData",
            ],
        )
    }
    /// Reuse the literal declaration reader for other explicitly selected classes.
    /// Calling this does not evaluate callbacks or make conditional fields static.
    pub fn read_classes(&mut self, source: &str, origin: &str, classes: &[&str]) -> Result<()> {
        let t = lex(source)?;
        let mut i = 0;
        let mut depth = 0i32;
        while i < t.len() {
            if depth == 0 && control(&t[i]) {
                let start = i;
                skip_statement(&t, &mut i, 0)?;
                if t[start..i].iter().any(|t| t.atom("datablock")) {
                    self.diagnostics.push(format!(
                        "{origin}: conditional datablock declarations require explicit adaptation"
                    ));
                }
                continue;
            }
            if depth == 0
                && t[i].atom("datablock")
                && t.get(i + 1)
                    .is_some_and(|c| classes.iter().any(|n| c.atom(n)))
            {
                i += 1;
                let class = t[i].literal()?.to_lowercase();
                i += 1;
                symbol(&t, &mut i, '(')?;
                let name = literal(&t, &mut i)?;
                let mut fields = if t.get(i) == Some(&Token::Symbol(':')) {
                    i += 1;
                    let parent = literal(&t, &mut i)?;
                    let parent = self
                        .entries
                        .iter()
                        .find(|d| d.name.eq_ignore_ascii_case(&parent))
                        .context("Missing effect parent")?;
                    ensure!(parent.class == class, "Effect parent class mismatch");
                    parent.fields.clone()
                } else {
                    BTreeMap::new()
                };
                symbol(&t, &mut i, ')')?;
                symbol(&t, &mut i, '{')?;
                while t.get(i) != Some(&Token::Symbol('}')) {
                    let mut key = literal(&t, &mut i)?.to_lowercase();
                    if t.get(i) == Some(&Token::Symbol('[')) {
                        i += 1;
                        let index: usize = literal(&t, &mut i)?.parse()?;
                        ensure!(index < 32, "Effect array index too large");
                        symbol(&t, &mut i, ']')?;
                        key = format!("{key}[{index}]");
                    }
                    symbol(&t, &mut i, '=')?;
                    let mut value = literal(&t, &mut i)?;
                    // Core emitter-node declarations use literal ratios such as
                    // 1/20. This bounded numeric form is not script evaluation.
                    if key == "timemultiple" && t.get(i) == Some(&Token::Symbol('/')) {
                        i += 1;
                        let numerator: f32 = value.parse()?;
                        let denominator: f32 = literal(&t, &mut i)?.parse()?;
                        let ratio = numerator / denominator;
                        ensure!(
                            ratio.is_finite() && ratio > 0.0,
                            "Invalid emitter-node ratio"
                        );
                        value = ratio.to_string();
                    }
                    if matches!(key.as_str(), "texturename" | "flarebitmap") && !value.is_empty() {
                        value = crate::effects::texture_path(value, origin)?;
                    }
                    symbol(&t, &mut i, ';')?;
                    fields.insert(key, value);
                }
                i += 1;
                symbol(&t, &mut i, ';')?;
                ensure!(
                    !self
                        .entries
                        .iter()
                        .any(|d| d.name.eq_ignore_ascii_case(&name)),
                    "Duplicate effect {name}"
                );
                ensure!(self.entries.len() < 4096, "Too many effects");
                self.entries.push(Declaration {
                    class,
                    name,
                    fields,
                    source: origin.into(),
                });
                continue;
            }
            if depth == 0
                && let Token::Atom(atom) = &t[i]
                && let Some((name, field)) = atom.split_once('.')
                && let Some(d) = self
                    .entries
                    .iter_mut()
                    .find(|d| d.name.eq_ignore_ascii_case(name))
                && t.get(i + 1) == Some(&Token::Symbol('='))
            {
                // Only explicitly supported top-level naming assignments. No eval,
                // expressions, conditionals or source callbacks are evaluated.
                ensure!(
                    field.eq_ignore_ascii_case("uiname"),
                    "Unsupported effect override {atom}"
                );
                i += 2;
                let value = literal(&t, &mut i)?;
                symbol(&t, &mut i, ';')?;
                d.fields.insert("uiname".into(), value);
                continue;
            }
            match t[i] {
                Token::Symbol('{') => depth += 1,
                Token::Symbol('}') => depth -= 1,
                _ => {}
            }
            ensure!(depth >= 0, "Unbalanced effect source");
            i += 1;
        }
        ensure!(depth == 0, "Unbalanced effect source");
        Ok(())
    }

    /// Expand only literal, unconditional include statements in source order.
    /// The loader owns path validation and archive bounds. Code inside functions
    /// and conditional blocks remains unexecuted.
    pub fn read_with_includes(
        &mut self,
        source: &str,
        origin: &str,
        loader: &mut IncludeLoader<'_>,
    ) -> Result<()> {
        self.includes(source, origin, loader, &mut Vec::new())
    }
    fn includes(
        &mut self,
        source: &str,
        origin: &str,
        loader: &mut IncludeLoader<'_>,
        active: &mut Vec<String>,
    ) -> Result<()> {
        ensure!(
            active.len() < 32 && !active.iter().any(|s| s.eq_ignore_ascii_case(origin)),
            "Cyclic or deep effect include"
        );
        active.push(origin.into());
        let tokens = lex(source)?;
        let mut depth = 0i32;
        let mut start = 0;
        let mut i = 0;
        let render = |slice: &[Token]| -> String {
            slice
                .iter()
                .map(|t| match t {
                    Token::Atom(s) => s.clone(),
                    Token::String(s) => serde_json::to_string(s).unwrap(),
                    Token::Symbol(c) => c.to_string(),
                })
                .collect::<Vec<_>>()
                .join(" ")
        };
        while i < tokens.len() {
            if depth == 0 && control(&tokens[i]) {
                skip_statement(&tokens, &mut i, 0)?;
                continue;
            }
            if depth == 0
                && tokens[i].atom("exec")
                && tokens.get(i + 1) == Some(&Token::Symbol('('))
                && let Some(Token::String(path)) = tokens.get(i + 2)
                && tokens.get(i + 3) == Some(&Token::Symbol(')'))
                && tokens.get(i + 4) == Some(&Token::Symbol(';'))
            {
                self.read(&render(&tokens[start..i]), origin)?;
                let (text, child) = loader(path, origin)?;
                self.includes(&text, &child, loader, active)?;
                i += 5;
                start = i;
                continue;
            }
            match tokens[i] {
                Token::Symbol('{') => depth += 1,
                Token::Symbol('}') => depth -= 1,
                _ => {}
            }
            i += 1;
        }
        self.read(&render(&tokens[start..]), origin)?;
        active.pop();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_class_selection_keeps_static_adapters_out_of_effect_defaults() {
        let source = r#"
            datablock StaticShapeData(LCD){shapeFile="~/data/shapes/LCD.dts";};
            function LCD::onAdd(){datablock StaticShapeData(Fake){};}
            datablock ParticleData(Cloud){lifetimeMS=100;};
        "#;
        let mut effects = Declarations::default();
        effects.read(source, "base/server/scripts/core.cs").unwrap();
        assert_eq!(effects.entries.len(), 1);
        assert_eq!(effects.entries[0].name, "Cloud");
        let mut shapes = Declarations::default();
        shapes
            .read_classes(source, "base/server/scripts/core.cs", &["StaticShapeData"])
            .unwrap();
        assert_eq!(shapes.entries.len(), 1);
        assert_eq!(shapes.entries[0].name, "LCD");
        assert_eq!(
            shapes.entries[0].fields["shapefile"],
            "~/data/shapes/LCD.dts"
        );
    }
    #[test]
    fn includes_keep_order_origin_and_skip_conditional_code() {
        let mut d = Declarations::default();
        d.read_with_includes(
            r#"exec("./base.cs");
            if(false) exec("./never.cs");
            if(false) Base.uiName="Wrong"; else {Base.uiName="Also ignored";}
            datablock ParticleData(Child:Base){uiName="Child";};
            datablock ParticleEmitterNodeData(Node){timeMultiple=1/20;};"#,
            "Add-Ons/Test/server.cs",
            &mut |path, _| {
                assert_eq!(path, "./base.cs");
                Ok((
                    r#"datablock ParticleData(Base){textureName="./cloud";};"#.into(),
                    "Add-Ons/Test/sub/base.cs".into(),
                ))
            },
        )
        .unwrap();
        assert_eq!(d.entries.len(), 3);
        assert_eq!(d.entries[1].fields["texturename"], "add-ons/test/sub/cloud");
        assert!(!d.entries[0].fields.contains_key("uiname"));
        assert_eq!(d.entries[2].fields["timemultiple"], "0.05");
        assert!(
            Declarations::default()
                .read_with_includes("exec(\"./self.cs\");", "self.cs", &mut |_, _| Ok((
                    "exec(\"./self.cs\");".into(),
                    "self.cs".into()
                )))
                .is_err()
        );
    }
    #[test]
    fn literal_inheritance_arrays_overrides_and_code_separation() {
        let mut d = Declarations::default();
        d.read(
            r#"function ignore(){datablock ParticleData(Fake){lifetimeMS=eval("x");};}
            datablock ParticleData(Base){gravityCoefficient=-0.7; colors[0]="1 0 0 1";};
            datablock ParticleData(Child:Base){lifetimeMS=100;};
            Child.uiName="Listed";"#,
            "fixture",
        )
        .unwrap();
        assert_eq!(d.entries.len(), 2);
        assert_eq!(d.entries[1].fields["gravitycoefficient"], "-0.7");
        assert_eq!(d.entries[1].fields["uiname"], "Listed");
        assert!(
            Declarations::default()
                .read("datablock ParticleData(B){lifetimeMS=1+2;};", "bad")
                .is_err()
        );
        assert!(
            Declarations::default()
                .read("datablock ParticleData(B){colors[99]=1;};", "bad")
                .is_err()
        );
        assert!(
            Declarations::default()
                .read("datablock ParticleData(B:Missing){};", "bad")
                .is_err()
        );
    }
}
