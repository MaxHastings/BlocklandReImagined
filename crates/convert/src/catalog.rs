//! Extract constant brick declarations only. No interpreter or script execution.
use anyhow::{Context, Result, bail, ensure};
use bri_content::brick::{Catalog, CatalogEntry, Face, Frame, Link, Reflection};
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
/// The value of a constant expression: literals and known globals joined
/// by `@`, `SPC`, `TAB` or `NL`. `None` for anything else.
fn constant(tokens: &[Token], globals: &Globals) -> Option<String> {
    let mut out = String::new();
    let mut i = 0;
    let mut joiner = Some("");
    while i < tokens.len() {
        let sep = joiner.take()?;
        out.push_str(sep);
        match &tokens[i] {
            Token::String(v) => {
                out.push_str(v);
                i += 1;
            }
            Token::Atom(v) if v.starts_with('$') => {
                let mut name = v.to_ascii_lowercase();
                i += 1;
                while tokens.get(i) == Some(&Token::Symbol(':'))
                    && tokens.get(i + 1) == Some(&Token::Symbol(':'))
                {
                    let Some(Token::Atom(part)) = tokens.get(i + 2) else {
                        return None;
                    };
                    name = format!("{name}::{}", part.to_ascii_lowercase());
                    i += 3;
                }
                out.push_str(globals.get(&name)?);
            }
            Token::Atom(v) if v.parse::<f64>().is_ok() => {
                out.push_str(v);
                i += 1;
            }
            _ => return None,
        }
        joiner = match tokens.get(i) {
            None => break,
            Some(Token::Symbol('@')) => Some(""),
            Some(t) if t.atom("SPC") => Some(" "),
            Some(t) if t.atom("TAB") => Some("\t"),
            Some(t) if t.atom("NL") => Some("\n"),
            Some(_) => return None,
        };
        i += 1;
    }
    joiner.is_none().then_some(out)
}
/// A global's value as an Add-On sets it at load (`$X = <text>;`), when it
/// is constant: literals, earlier globals in `globals`, `@`, `SPC`, `TAB`,
/// `NL`, and `filePath(expandFileName("./file.cs"))` or
/// `filePath($Con::File)`, the declaring script's folder `script_directory`.
pub fn constant_global(text: &str, script_directory: &str, globals: &Globals) -> Option<String> {
    let tokens = lex(text).ok()?;
    // `filePath(expandFileName("./x"))` and `filePath($Con::File)`: the
    // folder the script lives in.
    let mut folded = Vec::with_capacity(tokens.len());
    let mut i = 0;
    while i < tokens.len() {
        let running = tokens.get(i + 2).is_some_and(|t| t.atom("$Con"))
            && tokens.get(i + 3) == Some(&Token::Symbol(':'))
            && tokens.get(i + 4) == Some(&Token::Symbol(':'))
            && tokens.get(i + 5).is_some_and(|t| t.atom("File"));
        let expanded = tokens.get(i + 2).is_some_and(|t| t.atom("expandFileName"))
            && tokens.get(i + 3) == Some(&Token::Symbol('('))
            && matches!(tokens.get(i + 4), Some(Token::String(p)) if p.starts_with("./") && !p[2..].contains('/'))
            && tokens.get(i + 5) == Some(&Token::Symbol(')'));
        if tokens[i].atom("filePath")
            && tokens.get(i + 1) == Some(&Token::Symbol('('))
            && (running || expanded)
            && tokens.get(i + 6) == Some(&Token::Symbol(')'))
        {
            folded.push(Token::String(script_directory.to_string()));
            i += 7;
        } else {
            folded.push(tokens[i].clone());
            i += 1;
        }
    }
    constant(&folded, globals)
}
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
/// A mirror brick's `reflection*` fields (not in v20): `reflectionFaces`
/// names its mirrored sides ("north south"); the rest are optional. The
/// brick's size is checked when the catalog loads.
fn reflection(fields: &mut Fields) -> Result<Option<Reflection>> {
    let faces = take(fields, "reflectionfaces")?;
    let depth = number(fields, "reflectiondepth")?;
    let inset = number(fields, "reflectioninset")?;
    let strength = number(fields, "reflectionstrength")?;
    let tint = take(fields, "reflectiontint")?;
    let Some(faces) = faces else {
        ensure!(
            depth.is_none() && inset.is_none() && strength.is_none() && tint.is_none(),
            "reflection fields need reflectionFaces"
        );
        return Ok(None);
    };
    Ok(Some(Reflection {
        faces: sides(&faces, "reflectionFaces")?,
        depth: depth.unwrap_or(0.0),
        inset: inset.unwrap_or(0.0),
        tint: colour(tint, "reflectionTint")?.unwrap_or([1.0; 3]),
        strength: strength.unwrap_or(1.0),
    }))
}
/// A linked brick's `link*` fields (portals; not in v20): `linkFaces` names
/// its open sides and `linkName` the stem of the names placing a pair gives;
/// the rest are optional. Checked again when the catalog loads.
fn link(fields: &mut Fields) -> Result<Option<Link>> {
    let faces = take(fields, "linkfaces")?;
    let name = take(fields, "linkname")?;
    let depth = number(fields, "linkdepth")?;
    let inset = number(fields, "linkinset")?;
    let frame = take(fields, "linkframe")?;
    let tint = take(fields, "linktint")?;
    let idle = take(fields, "linkidle")?;
    let pass = take(fields, "linkpass")?;
    let Some(faces) = faces else {
        ensure!(
            name.is_none()
                && depth.is_none()
                && inset.is_none()
                && frame.is_none()
                && tint.is_none()
                && idle.is_none()
                && pass.is_none(),
            "link fields need linkFaces"
        );
        return Ok(None);
    };
    let pass = match pass.as_deref() {
        None | Some("0" | "false") => false,
        Some("1" | "true") => true,
        _ => bail!("Invalid boolean field linkPass"),
    };
    Ok(Some(Link {
        faces: sides(&faces, "linkFaces")?,
        depth: depth.unwrap_or(0.0),
        inset: inset.unwrap_or(0.0),
        tint: colour(tint, "linkTint")?.unwrap_or([1.0; 3]),
        idle: colour(idle, "linkIdle")?.unwrap_or(Link::haze()),
        pass,
        frame: frame
            .map(|f| frame_widths(&f))
            .transpose()?
            .unwrap_or_default(),
        name: name.context("linkFaces needs linkName")?,
    }))
}
/// `stretchSize` (not in v20): the brick at another size than its
/// `brickFile`, "width depth height" in studs, studs and plates.
fn stretch(fields: &mut Fields) -> Result<Option<[u32; 3]>> {
    take(fields, "stretchsize")?
        .map(|value| {
            let values: Vec<u32> = value
                .split_whitespace()
                .map(|v| v.parse().ok().filter(|v| *v > 0))
                .collect::<Option<_>>()
                .context("Invalid stretchSize")?;
            <[u32; 3]>::try_from(values)
                .ok()
                .context("stretchSize needs three whole numbers (width depth height)")
        })
        .transpose()
}
/// `linkFrame`: one width for every edge, or "sides top bottom".
fn frame_widths(value: &str) -> Result<Frame> {
    let values: Vec<f32> = value
        .split_whitespace()
        .map(|v| v.parse().ok().filter(|v: &f32| v.is_finite()))
        .collect::<Option<_>>()
        .context("Invalid linkFrame")?;
    Ok(match values[..] {
        [width] => Frame::even(width),
        [sides, top, bottom] => Frame { sides, top, bottom },
        _ => bail!("linkFrame needs one number or three (sides top bottom)"),
    })
}
fn number(fields: &mut Fields, key: &str) -> Result<Option<f32>> {
    take(fields, key)?
        .map(|v| {
            v.trim()
                .parse()
                .ok()
                .filter(|v: &f32| v.is_finite())
                .with_context(|| format!("Invalid number field {key}"))
        })
        .transpose()
}
fn sides(faces: &str, key: &str) -> Result<Vec<Face>> {
    faces
        .split_whitespace()
        .map(|face| {
            Ok(match face.to_ascii_lowercase().as_str() {
                "top" => Face::Top,
                "bottom" => Face::Bottom,
                "north" => Face::North,
                "east" => Face::East,
                "south" => Face::South,
                "west" => Face::West,
                _ => bail!("Invalid {key} side {face}"),
            })
        })
        .collect()
}
fn colour(value: Option<String>, key: &str) -> Result<Option<[f32; 3]>> {
    value
        .map(|value| {
            let values: Vec<f32> = value
                .split_whitespace()
                .map(|v| v.parse().ok().filter(|v: &f32| v.is_finite()))
                .collect::<Option<_>>()
                .with_context(|| format!("Invalid {key}"))?;
            values
                .try_into()
                .ok()
                .with_context(|| format!("{key} needs three numbers"))
        })
        .transpose()
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
    read_with_globals(source, virtual_directory, parents, &Globals::new())
}
/// Script globals an Add-On sets at load to constant values, by lower-case
/// name with its `$` (`$slayer::server::path`), as [`constant_global`]
/// evaluates them.
pub type Globals = BTreeMap<String, String>;
/// Like `read_with_parents`, but a field may also be a constant expression
/// over `globals`: `category = $Slayer::Server::Bricks::Category;` or
/// `brickFile = $Slayer::Server::Path @ "/shapes/node.blb";`. Still no
/// interpreter: only literals, known globals and `@`, `SPC`, `TAB`, `NL`.
pub fn read_with_globals(
    source: &str,
    virtual_directory: &str,
    parents: &str,
    globals: &Globals,
) -> Result<Catalog> {
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
        for tokens in fields.values_mut() {
            if tokens.len() > 1
                && let Some(value) = constant(tokens, globals)
            {
                *tokens = vec![Token::String(value)];
            }
        }
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
        let reflection = reflection(&mut fields)?;
        let link = link(&mut fields)?;
        let stretch = stretch(&mut fields)?;
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
            reflection,
            link,
            stretch,
            swap: None,
            bot: None,
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
    fn the_running_scripts_folder_is_a_constant() {
        let mut globals = Globals::new();
        let path = constant_global(r#"filePath($Con::File) @ "/""#, "Add-Ons/A", &globals);
        assert_eq!(path.as_deref(), Some("Add-Ons/A/"));
        globals.insert("$a::path".into(), path.unwrap());
        assert_eq!(
            constant_global(r#"$A::Path @ "x.dts""#, "Add-Ons/A", &globals).as_deref(),
            Some("Add-Ons/A/x.dts")
        );
    }
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
    fn reflection_fields_make_a_mirror_brick() {
        let catalog = read_at(
            r#"datablock fxDTSBrickData(Mirror) {brickFile="./mirror.blb";uiName="Mirror";
            reflectionFaces="north South";reflectionDepth=0.5;reflectionInset=0.1;
            reflectionTint="0.9 0.9 1";};"#,
            "Add-Ons/Brick_Mirror",
        )
        .unwrap();
        let brick = &catalog.bricks[0];
        assert_eq!(
            brick.reflection,
            Some(Reflection {
                faces: vec![Face::North, Face::South],
                depth: 0.5,
                inset: 0.1,
                tint: [0.9, 0.9, 1.0],
                strength: 1.0,
            })
        );
        assert!(brick.other_properties.is_empty());
        let portal = read_at(
            r#"datablock fxDTSBrickData(Portal) {brickFile="./p.blb";uiName="Portal";
            linkFaces="north south";linkName="Portal";linkDepth=0.5;linkPass=1;
            linkFrame="0.05 0.05 0.2";stretchSize="8 1 30";};"#,
            "Add-Ons/Brick_Portal",
        )
        .unwrap();
        let link = portal.bricks[0].link.as_ref().unwrap();
        assert_eq!(
            (link.faces.len(), link.name.as_str(), link.pass, link.frame),
            (
                2,
                "Portal",
                true,
                Frame {
                    sides: 0.05,
                    top: 0.05,
                    bottom: 0.2
                }
            )
        );
        assert_eq!(portal.bricks[0].stretch, Some([8, 1, 30]));
        assert!(portal.bricks[0].other_properties.is_empty());
        for bad in [
            r#"stretchSize="8 1";"#,
            r#"stretchSize="8 0 30";"#,
            r#"linkFaces="north";"#,
            r#"linkName="Portal";"#,
            r#"linkFaces="north";linkName="P";linkPass=maybe;"#,
            r#"linkFaces="north";linkName="P";linkFrame="0.1 0.2";"#,
            r#"reflectionFaces="up";"#,
            r#"reflectionFaces="north";reflectionTint="1 1";"#,
            r#"reflectionDepth=0.5;"#,
        ] {
            let source =
                format!(r#"datablock fxDTSBrickData(M) {{brickFile="./m.blb";uiName="M";{bad}}};"#);
            assert!(read_at(&source, "Add-Ons/Brick_Mirror").is_err(), "{bad}");
        }
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
    fn bls_fixture() -> String {
        format!(
            "This is a Blockland save file.\n1\nDescription\n{}Linecount 1\nMissing Brick\" 1 2 3 1 0 2  0 0 1 0 1\n",
            "1 0.5 0 1\n".repeat(64)
        )
    }
    #[test]
    fn hidden_state_variants_keep_identity_but_last_name_loads_from_bls() {
        let catalog = read_at(r#"
            datablock fxDTSBrickData(Open) {brickFile="./open.blb";uiName="Chest";iconName="";};
            datablock fxDTSBrickData(Closed) {brickFile="./closed.blb";uiName="Chest";category="Special";subCategory="Interactive";};
        "#, "Add-Ons/Chest").unwrap();
        assert!(!catalog.bricks[0].selectable());
        assert!(catalog.bricks[1].selectable());
        assert_eq!(catalog.bricks[0].icon_source, "");
        let world = bri_bls::bls::read(
            bls_fixture().replace("Missing Brick", "Chest").as_bytes(),
            &catalog,
            "test",
            "map/test",
        )
        .unwrap();
        assert_eq!(
            world.bricks[&1].definition,
            bri_world::ContentRef::Resolved("v20/brick/closed".into())
        );
        assert_eq!(catalog.bricks[0].mesh_id, "v20/add-ons/chest/open.blb");
    }
}
