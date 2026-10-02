//! An Add-On's help file (`.hfl`) as a Help dialog page: Torque ML with
//! the custom tags Slayer's `Support_TMLParser` adds (`<b>`, `<i>`, `<u>`,
//! `<h1>`-`<h3>`, `<size:n>`, `<color:..>`..`</color>`, `<ol>`, `<ul>` and
//! `<li>`) turned into the ML the UI draws. Anchors (`<tag:id>`) and links
//! (`<a:#id>`) stay as they are.

/// One list being written: `ol` counts its items.
enum List {
    Ordered(u32),
    Bullets,
}

/// `parseCustomTML(text, obj, "default")` over a whole file, with the
/// font state `Support_TMLParser` keeps per control.
pub fn page_text(src: &str) -> String {
    const BULLET_INDENT: u32 = 2;
    const TEXT_INDENT: u32 = 2;
    let src = src.replace('\r', "");
    let mut out = String::with_capacity(src.len());
    let mut face = String::from("arial");
    let mut size = String::from("15");
    let mut lists: Vec<List> = Vec::new();
    let mut rest = src.as_str();
    while let Some(i) = rest.find('<') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let Some(end) = tail[1..]
            .find(['>', '<'])
            .map(|e| e + 1)
            .filter(|&e| tail.as_bytes()[e] == b'>')
        else {
            out.push('<');
            rest = &tail[1..];
            continue;
        };
        let body = &tail[1..end];
        rest = &tail[end + 1..];
        let mut parts = body.split(':');
        let name = parts.next().unwrap_or("").to_ascii_lowercase();
        let value = |n: usize| body.split(':').nth(n).unwrap_or("");
        let level = lists.len() as u32;
        let bullet_at = |level: u32| level * BULLET_INDENT + level.saturating_sub(1) * TEXT_INDENT;
        match name.as_str() {
            "font" => {
                face = value(1).to_owned();
                size = value(2).to_owned();
                out.push_str(&format!("<{body}>"));
            }
            "b" => {
                let bold = if face.to_ascii_lowercase().contains("bold") {
                    "arial bold:15".to_owned()
                } else {
                    format!("{face} bold:{size}")
                };
                out.push_str(&format!("<spush><font:{bold}>"));
            }
            "i" => {
                if face.to_ascii_lowercase().contains("italic") {
                    out.push_str("<spush>");
                } else {
                    out.push_str(&format!("<spush><font:{face} italic:{size}>"));
                }
            }
            // The underline was a link with no address; it draws plain.
            "u" => out.push_str("<spush>"),
            "/b" | "/i" | "/u" | "/size" | "/color" => out.push_str("<spop>"),
            "size" => out.push_str(&format!("<spush><font:{face}:{}>", value(1))),
            "color" | "colorhex" => out.push_str(&format!("<spush><color:{}>", value(1))),
            "/just" => out.push_str("<just:left>"),
            "h1" => out.push_str("<spush><font:arial bold:24>"),
            "h2" => out.push_str("<spush><font:arial bold:20>"),
            "h3" => out.push_str("<spush><font:arial bold:17>"),
            "/h1" | "/h2" | "/h3" => out.push_str("<spop><br>"),
            "ol" => lists.push(List::Ordered(
                value(1).parse::<u32>().map_or(0, |n| n.saturating_sub(1)),
            )),
            "ul" => lists.push(List::Bullets),
            "/ol" | "/ul" => {
                lists.pop();
                if lists.is_empty() {
                    out.push_str("<lmargin%:0>");
                }
            }
            "li" => {
                let at = bullet_at(level);
                let text_at = at + TEXT_INDENT;
                match lists.last_mut() {
                    Some(List::Bullets) => out.push_str(&format!(
                        "<br><lmargin%:{at}><spush><font:arial bold:15>+<spop><lmargin%:{text_at}>"
                    )),
                    Some(List::Ordered(n)) => {
                        *n += 1;
                        out.push_str(&format!(
                            "<br><lmargin%:{at}><spush><font:arial bold:15>{n}.<spop><lmargin%:{text_at}>"
                        ));
                    }
                    None => {}
                }
            }
            "/li" if level > 0 => {
                out.push_str(&format!(
                    "<lmargin%:{}>",
                    BULLET_INDENT * (level - 1) + TEXT_INDENT
                ));
            }
            "/li" => {}
            // Torque's own ML tags, and anything else, as written.
            _ => out.push_str(&format!("<{body}>")),
        }
    }
    out.push_str(rest);
    out.replace('\t', "    ").trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_tags_become_the_ml_the_help_dialog_draws() {
        let src = "<font:Arial:14><h1>Guide</h1>Press <b>F</b> and <i>go</i>.\r\n<ul><li>one</li><li>two</li></ul><ol:3><li>c</li></ol>\
                   <a:#teams>Teams</a><tag:teams><color:ff0000>red</color><size:20>big</size><just:center>x</just>";
        assert_eq!(
            page_text(src),
            "<font:Arial:14><spush><font:arial bold:24>Guide<spop><br>Press <spush><font:Arial bold:14>F<spop> and \
             <spush><font:Arial italic:14>go<spop>.\n\
             <br><lmargin%:2><spush><font:arial bold:15>+<spop><lmargin%:4>one<lmargin%:2>\
             <br><lmargin%:2><spush><font:arial bold:15>+<spop><lmargin%:4>two<lmargin%:2><lmargin%:0>\
             <br><lmargin%:2><spush><font:arial bold:15>3.<spop><lmargin%:4>c<lmargin%:2><lmargin%:0>\
             <a:#teams>Teams</a><tag:teams><spush><color:ff0000>red<spop><spush><font:Arial:20>big<spop>\
             <just:center>x<just:left>"
        );
        // A stray `<` is text.
        assert_eq!(page_text("a < b <3"), "a < b <3");
    }
}
