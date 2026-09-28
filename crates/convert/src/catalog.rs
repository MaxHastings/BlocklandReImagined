//! Extract constant brick declarations only. No interpreter or script execution.
use anyhow::{Context, Result, bail, ensure};
use bri_content::brick::{Catalog, CatalogEntry};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Token {
    Atom(String),
    String(String),
    Symbol(char),
}
impl Token {
    pub(crate) fn atom(&self, expected: &str) -> bool {
        matches!(self,Self::Atom(v) if v.eq_ignore_ascii_case(expected))
    }
    pub(crate) fn literal(&self) -> Result<String> {
        match self {
            Self::Atom(v) | Self::String(v) => Ok(v.clone()),
            _ => bail!("Expected literal, found {self:?}"),
        }
    }
}
pub(crate) fn lex(source: &str) -> Result<Vec<Token>> {
    ensure!(source.len() <= 16 * 1024 * 1024, "Script too large");
    let chars: Vec<char> = source.chars().collect();
    let mut i = 0;
    let mut tokens = Vec::new();
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'*') {
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            ensure!(i + 1 < chars.len(), "Unterminated block comment");
            i += 2;
            continue;
        }
        if c == '"' || c == '\'' {
            let quote = c;
            i += 1;
            let mut s = String::new();
            while i < chars.len() && chars[i] != quote {
                if chars[i] == '\\' {
                    i += 1;
                    let escaped = *chars.get(i).context("Truncated escape")?;
                    match escaped {
                        'n' => s.push('\n'),
                        'r' => s.push('\r'),
                        't' => s.push('\t'),
                        '\\' => s.push('\\'),
                        '"' => s.push('"'),
                        '\'' => s.push('\''),
                        other => {
                            s.push('\\');
                            s.push(other);
                        }
                    }
                } else {
                    s.push(chars[i]);
                }
                i += 1;
            }
            ensure!(i < chars.len(), "Unterminated string");
            i += 1;
            tokens.push(Token::String(s));
            continue;
        }
        if c.is_alphanumeric() || "_$%~".contains(c) {
            let start = i;
            i += 1;
            while i < chars.len() && (chars[i].is_alphanumeric() || "_$%~.".contains(chars[i])) {
                i += 1;
            }
            tokens.push(Token::Atom(chars[start..i].iter().collect()));
        } else {
            tokens.push(Token::Symbol(c));
            i += 1;
        }
    }
    Ok(tokens)
}

#[derive(Clone)]
struct Declaration {
    name: String,
    parent: Option<String>,
    fields: BTreeMap<String, Vec<Token>>,
}
fn expect(tokens: &[Token], cursor: &mut usize, symbol: char) -> Result<()> {
    ensure!(
        tokens.get(*cursor) == Some(&Token::Symbol(symbol)),
        "Expected {symbol} at token {}",
        cursor
    );
    *cursor += 1;
    Ok(())
}
fn declarations(tokens: &[Token]) -> Result<Vec<Declaration>> {
    let mut cursor = 0;
    let mut depth = 0_i32;
    let mut declarations = Vec::new();
    while cursor < tokens.len() {
        if depth == 0
            && tokens[cursor].atom("datablock")
            && tokens
                .get(cursor + 1)
                .is_some_and(|t| t.atom("fxDTSBrickData"))
        {
            cursor += 2;
            expect(tokens, &mut cursor, '(')?;
            let name = tokens
                .get(cursor)
                .context("Missing datablock name")?
                .literal()?;
            cursor += 1;
            let parent = if tokens.get(cursor) == Some(&Token::Symbol(':')) {
                cursor += 1;
                let p = tokens.get(cursor).context("Missing parent")?.literal()?;
                cursor += 1;
                Some(p.to_lowercase())
            } else {
                None
            };
            expect(tokens, &mut cursor, ')')?;
            expect(tokens, &mut cursor, '{')?;
            let mut fields = BTreeMap::new();
            while tokens.get(cursor) != Some(&Token::Symbol('}')) {
                let field = tokens
                    .get(cursor)
                    .context("Unterminated datablock")?
                    .literal()?
                    .to_lowercase();
                cursor += 1;
                expect(tokens, &mut cursor, '=')?;
                let start = cursor;
                while tokens.get(cursor) != Some(&Token::Symbol(';')) {
                    ensure!(
                        cursor < tokens.len() && tokens[cursor] != Token::Symbol('}'),
                        "Unterminated field {field}"
                    );
                    cursor += 1;
                }
                ensure!(cursor > start, "Empty field {field}");
                fields.insert(field, tokens[start..cursor].to_vec());
                cursor += 1;
            }
            cursor += 1;
            expect(tokens, &mut cursor, ';')?;
            declarations.push(Declaration {
                name,
                parent,
                fields,
            });
        } else {
            match tokens[cursor] {
                Token::Symbol('{') => depth += 1,
                Token::Symbol('}') => depth -= 1,
                _ => {}
            }
            ensure!(depth >= 0, "Unbalanced script braces");
            cursor += 1;
        }
    }
    ensure!(depth == 0, "Unbalanced script braces");
    Ok(declarations)
}
type Fields = BTreeMap<String, Vec<Token>>;
fn resolve(
    name: &str,
    declarations: &BTreeMap<String, Declaration>,
    active: &mut BTreeSet<String>,
) -> Result<Fields> {
    ensure!(
        active.len() < 64 && active.insert(name.into()),
        "Cyclic or overly deep brick inheritance"
    );
    let declaration = declarations
        .get(name)
        .with_context(|| format!("Unknown parent {name}"))?;
    let mut fields = match &declaration.parent {
        Some(p) => resolve(p, declarations, active)?,
        None => BTreeMap::new(),
    };
    fields.extend(declaration.fields.clone());
    active.remove(name);
    Ok(fields)
}
fn take(fields: &mut Fields, key: &str) -> Result<Option<String>> {
    fields
        .remove(key)
        .map(|v| {
            ensure!(
                v.len() == 1,
                "Dynamic expression in required field {key}: {v:?}"
            );
            v[0].literal()
        })
        .transpose()
}
fn boolean(fields: &mut Fields, key: &str, default: bool) -> Result<bool> {
    match take(fields, key)?.as_deref() {
        None => Ok(default),
        Some("1" | "true") => Ok(true),
        Some("0" | "false") => Ok(false),
        _ => bail!("Invalid boolean field {key}"),
    }
}
fn path(value: String) -> Result<String> {
    let value = value.replace('\\', "/");
    let value = value
        .strip_prefix("~/")
        .map_or(value.clone(), |tail| format!("base/{tail}"));
    ensure!(
        !value.starts_with('/')
            && !value.contains(':')
            && value
                .split('/')
                .all(|p| !p.is_empty() && p != ".." && p != "."),
        "Invalid source asset path {value}"
    );
    Ok(value.to_lowercase())
}
pub fn read_stock(source: &str) -> Result<Catalog> {
    read_at(source, "base")
}
/// Relative asset paths resolve against the declaring script's virtual folder.
pub fn read_at(source: &str, virtual_directory: &str) -> Result<Catalog> {
    read_with_parents(source, virtual_directory, "")
}
/// Like `read_at`, but parents may also come from `parents`: declarations
/// another package owns (a community Add-On's brick inheriting a base brick).
/// Only `source`'s bricks are returned.
pub fn read_with_parents(source: &str, virtual_directory: &str, parents: &str) -> Result<Catalog> {
    let directory = path(virtual_directory.into())?;
    let source_path = |value: String| {
        path(
            value
                .strip_prefix("./")
                .map_or(value.clone(), |p| format!("{directory}/{p}")),
        )
    };
    let parsed = declarations(&lex(source)?)?;
    let mut map = BTreeMap::new();
    let mut order = Vec::new();
    for declaration in parsed {
        let key = declaration.name.to_lowercase();
        ensure!(!map.contains_key(&key), "Repeated datablock {key}");
        order.push(key.clone());
        map.insert(key, declaration);
    }
    let mut all = map.clone();
    for declaration in declarations(&lex(parents)?)? {
        all.entry(declaration.name.to_lowercase())
            .or_insert(declaration);
    }
    let mut bricks = Vec::new();
    for key in order {
        let mut fields = resolve(&key, &all, &mut BTreeSet::new())?;
        let mesh = source_path(take(&mut fields, "brickfile")?.context("Missing brickFile")?)?;
        let display_name = take(&mut fields, "uiname")?.context("Missing uiName")?;
        let category = take(&mut fields, "category")?.unwrap_or_default();
        let subcategory = take(&mut fields, "subcategory")?.unwrap_or_default();
        let collision_source = take(&mut fields, "collisionshapename")?
            .map(&source_path)
            .transpose()?;
        let icon_source = take(&mut fields, "iconname")?
            .filter(|value| !value.is_empty())
            .map(&source_path)
            .transpose()?
            .unwrap_or_default();
        let print_aspect_ratio = take(&mut fields, "printaspectratio")?;
        let orientation_fix = take(&mut fields, "orientationfix")?
            .unwrap_or("0".into())
            .parse()?;
        ensure!(orientation_fix <= 3, "Invalid orientationFix");
        let can_cover = boolean(&mut fields, "cancover", true)?;
        let indestructible = boolean(&mut fields, "indestructable", false)?;
        let special_kind = take(&mut fields, "specialbricktype")?;
        let other_properties = fields
            .into_iter()
            .map(|(k, v)| {
                (
                    k,
                    v.iter()
                        .map(|t| match t {
                            Token::Atom(a) => a.clone(),
                            Token::String(s) => format!("{s:?}"),
                            Token::Symbol(c) => c.to_string(),
                        })
                        .collect::<Vec<_>>()
                        .join(" "),
                )
            })
            .collect();
        bricks.push(CatalogEntry {
            id: format!("v20/brick/{key}"),
            display_name,
            category,
            subcategory,
            mesh_id: format!("v20/{mesh}"),
            collision_source,
            icon_source,
            print_aspect_ratio,
            orientation_fix,
            can_cover,
            indestructible,
            special_kind,
            other_properties,
        });
    }
    ensure!(!bricks.is_empty(), "No static brick declarations found");
    Ok(Catalog {
        schema_version: 1,
        bricks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parents_from_another_package_are_inherited_but_not_returned() {
        let parents = r#"datablock fxDTSBrickData(brick2x2DiscData) {brickFile="base/data/bricks/2x2disc.blb";category="Bricks";subCategory="Round";uiName="2x2 Disc";};"#;
        let catalog = read_with_parents(
            r#"datablock fxDTSBrickData(Pad : brick2x2DiscData) {uiName="Pad";};"#,
            "Add-Ons/Brick_Pad",
            parents,
        )
        .unwrap();
        assert_eq!(catalog.bricks.len(), 1);
        assert_eq!(
            catalog.bricks[0].mesh_id,
            "v20/base/data/bricks/2x2disc.blb"
        );
        assert_eq!(catalog.bricks[0].category, "Bricks");
    }
    #[test]
    fn addon_relative_assets_resolve_at_the_declaring_folder() {
        let catalog=read_at(r#"datablock fxDTSBrickData(Cube) {brickFile="./8x Cube.blb";uiName="8x Cube";iconName="./8x Cube";};"#,"Add-Ons/Brick_Large_Cubes").unwrap();
        assert_eq!(
            catalog.bricks[0].mesh_id,
            "v20/add-ons/brick_large_cubes/8x cube.blb"
        );
        assert_eq!(
            catalog.bricks[0].icon_source,
            "add-ons/brick_large_cubes/8x cube"
        );
        assert!(
            read_at(
                r#"datablock fxDTSBrickData(Cube) {brickFile="./../bad.blb";uiName="bad";};"#,
                "Add-Ons/Brick_Large_Cubes"
            )
            .is_err()
        );
    }
    #[test]
    fn inheritance_comments_and_string_braces() {
        let source = r#"// datablock fxDTSBrickData(fake) {};
        function ignored() { echo("}"); }
        datablock fxDTSBrickData(Base) { brickFile="~/data/a.blb"; uiName="Base }"; category="Bricks"; canCover=0; };
        datablock fxDTSBrickData(Child : Base) { uiName="Child"; orientationFix=3; brickType=$TYPE::SPECIAL; };
        "#;
        let c = read_stock(source).unwrap();
        assert_eq!(c.bricks.len(), 2);
        assert_eq!(c.bricks[1].mesh_id, "v20/base/data/a.blb");
        assert_eq!(c.bricks[1].category, "Bricks");
        assert!(!c.bricks[1].can_cover);
        assert_eq!(c.bricks[1].orientation_fix, 3);
        assert!(c.bricks[1].other_properties.contains_key("bricktype"));
    }
    #[test]
    fn rejects_cycles_dynamic_required_fields_and_traversal() {
        for source in [
            r#"datablock fxDTSBrickData(A:B) {}; datablock fxDTSBrickData(B:A) {};"#,
            r#"datablock fxDTSBrickData(A) {brickFile=eval("x");uiName="a";};"#,
            r#"datablock fxDTSBrickData(A) {brickFile="../x";uiName="a";};"#,
        ] {
            assert!(read_stock(source).is_err());
        }
    }
}
