//! Chat HUD history and paging (`NewChatSO`, c:14724–15190) and the chat
//! input rules (`newMessageHud`, `NMH_Type::send`, c:14531–14705).

use crate::api::{ChatChannel, UiAction};

#[derive(Debug, Clone, PartialEq)]
pub struct ChatLine {
    /// Markup text (`\cN` as U+E000+N, `<a:…>` kept).
    pub text: String,
    pub time_ms: u64,
}

#[derive(Debug, Clone)]
pub struct ChatModel {
    pub lines: Vec<ChatLine>,
    pub cache_lines: usize,
    pub max_lines: usize,
    pub line_time_ms: i64,
    /// Paging end (exclusive) into `lines`; `None` = showing the latest.
    pub page_end: Option<usize>,
    /// Absolute index of `lines[0]` (lines dropped from the cache).
    dropped: usize,
}

impl ChatModel {
    /// Stock clamps from `newChatHud_Init`.
    pub fn new(cache_lines: usize, max_lines: usize, line_time_ms: i64) -> Self {
        ChatModel {
            lines: Vec::new(),
            cache_lines: cache_lines.clamp(100, 50_000),
            max_lines: max_lines.clamp(4, 100),
            line_time_ms: line_time_ms.min(30_000),
            page_end: None,
            dropped: 0,
        }
    }

    /// `newChatHud_AddLine` + `NewChatSO::addLine`: newlines and `<br>`
    /// become spaces; an unterminated link is closed.
    pub fn add(&mut self, line: &str, now: u64) {
        let mut l = line.replace('\n', " ");
        for br in ["<br>", "<bR>", "<Br>", "<BR>"] {
            l = l.replace(br, " ");
        }
        if l.contains("<a:") {
            l.push_str("</a>");
        }
        let was_at_bottom = self.page_end == Some(self.lines.len());
        self.lines.push(ChatLine {
            text: l,
            time_ms: now,
        });
        if self.lines.len() > self.cache_lines {
            self.lines.remove(0);
            self.dropped += 1;
            if let Some(e) = self.page_end.as_mut() {
                *e = e
                    .saturating_sub(1)
                    .max(self.max_lines.min(self.lines.len()));
            }
        }
        if was_at_bottom {
            self.page_end = Some(self.lines.len());
        }
    }

    /// Lines currently shown by the HUD, oldest first.
    pub fn visible(&self, now: u64) -> Vec<&ChatLine> {
        if self.line_time_ms <= 0 {
            return Vec::new();
        }
        match self.page_end {
            None => {
                // displayLatest: newest lines younger than LineTime; the
                // loop stops at maxLines - 1 (so at most maxLines - 1 lines).
                let mut n = 0;
                for (i, l) in self.lines.iter().rev().enumerate() {
                    if now.saturating_sub(l.time_ms) as i64 > self.line_time_ms
                        || i == self.max_lines - 1
                    {
                        break;
                    }
                    n += 1;
                }
                self.lines[self.lines.len() - n..].iter().collect()
            }
            Some(end) => {
                let start = end.saturating_sub(self.max_lines);
                self.lines[start..end.min(self.lines.len())]
                    .iter()
                    .collect()
            }
        }
    }

    /// "VVV" scroll-down indicator.
    pub fn scrolled_up(&self) -> bool {
        self.page_end.is_some_and(|e| e != self.lines.len())
    }

    pub fn page_up(&mut self) {
        let len = self.lines.len();
        let m = self.max_lines;
        self.page_end = Some(match self.page_end {
            None => len,
            Some(e) if e <= m * 2 => {
                if len <= m {
                    e
                } else {
                    m
                }
            }
            Some(e) => e - m,
        });
    }

    pub fn page_down(&mut self) {
        match self.page_end {
            None => {}
            Some(e) if e == self.lines.len() => self.page_end = None,
            Some(e) => self.page_end = Some((e + self.max_lines).min(self.lines.len())),
        }
    }
}

/// Result of pressing Enter in the chat input.
#[derive(Debug, Clone, PartialEq)]
pub enum ChatSend {
    /// Close the input; nothing sent (empty text).
    Close,
    Send(UiAction),
    /// Blocked (looks like an auth key): show this warning box.
    Blocked {
        title: String,
        text: String,
    },
}

/// `NMH_Type::send` (c:14630).
pub fn chat_send(channel: ChatChannel, text: &str) -> ChatSend {
    if let Some(rest) = text.strip_prefix('/') {
        let rest: String = rest.chars().take(256).collect();
        let mut words = rest.split_whitespace();
        let Some(name) = words.next() else {
            return ChatSend::Close;
        };
        return ChatSend::Send(UiAction::ChatCommand {
            name: name.to_string(),
            args: words.map(str::to_string).collect(),
        });
    }
    if text.trim().is_empty() {
        return ChatSend::Close;
    }
    // Auth-key heuristic: dashes at relative offsets "5 10" from the first.
    if let Some(first) = text.find('-') {
        let chain: String = text[first + 1..]
            .match_indices('-')
            .map(|(p, _)| format!(" {}", p + 1))
            .collect();
        if chain.contains("5 10") {
            return ChatSend::Blocked {
                title: "WARNING - CHAT BLOCKED".into(),
                text: "You just tried to say something that looks a lot like a Blockland Authentication key.\n\nDo not give out your key to anyone.".into(),
            };
        }
    }
    ChatSend::Send(UiAction::Chat {
        channel,
        text: text.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latest_lines_fade_and_cap() {
        let mut c = ChatModel::new(1000, 8, 6500);
        for i in 0..10 {
            c.add(&format!("line {i}"), i * 100);
        }
        let v = c.visible(1000);
        assert_eq!(v.len(), 7);
        assert_eq!(v.last().unwrap().text, "line 9");
        assert!(c.visible(20_000).is_empty());
    }

    #[test]
    fn paging() {
        let mut c = ChatModel::new(1000, 4, 6500);
        for i in 0..20 {
            c.add(&format!("l{i}"), 0);
        }
        c.page_up();
        assert!(!c.scrolled_up());
        c.page_up();
        assert!(c.scrolled_up());
        assert_eq!(c.visible(99_999).last().unwrap().text, "l15");
        c.page_down();
        assert_eq!(c.page_end, Some(20));
        c.add("new", 0);
        assert_eq!(c.page_end, Some(21));
        c.page_down();
        assert_eq!(c.page_end, None);
    }

    #[test]
    fn send_rules() {
        assert_eq!(chat_send(ChatChannel::Say, "   "), ChatSend::Close);
        assert_eq!(
            chat_send(ChatChannel::Say, "/sit now"),
            ChatSend::Send(UiAction::ChatCommand {
                name: "sit".into(),
                args: vec!["now".into()]
            })
        );
        assert!(matches!(
            chat_send(ChatChannel::Team, "ABCD-EFGH-IJKL"),
            ChatSend::Send(_)
        ));
        assert!(matches!(
            chat_send(ChatChannel::Say, "ABCD-EFGH-IJKL-MNOP"),
            ChatSend::Blocked { .. }
        ));
        let mut c = ChatModel::new(10, 8, 0);
        c.add("x<br>y\nz", 0);
        assert_eq!(c.lines[0].text, "x y z");
    }
}
