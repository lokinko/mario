//! Streaming wire parser and draft projection. Drafts never bypass report validation.
use crate::{models::inference::AnalysisEvent, ports::analysis::EventSender};
use crate::{AppError, AppResult};

pub type DeltaCallback = Box<dyn Fn(&str) + Send + Sync>;

pub(crate) fn status(sender: &Option<EventSender>, message: impl Into<String>) {
    if let Some(sender) = sender {
        let _ = sender.try_send(AnalysisEvent::Status {
            message: message.into(),
        });
    }
}

#[derive(Default)]
pub(crate) struct SseDecoder {
    buffer: Vec<u8>,
    data: Vec<String>,
}
impl SseDecoder {
    pub fn feed(&mut self, bytes: &[u8]) -> AppResult<Vec<String>> {
        self.buffer.extend_from_slice(bytes);
        if self.buffer.len() > 4 * 1024 * 1024 {
            return Err(AppError::Model("流式响应过大".into()));
        }
        let mut events = Vec::new();
        while let Some(end) = self.buffer.iter().position(|b| *b == b'\n') {
            let line = std::str::from_utf8(&self.buffer[..end])
                .map_err(|_| AppError::Model("流式响应编码无效".into()))?
                .trim_end_matches('\r')
                .to_string();
            self.buffer.drain(..=end);
            if line.is_empty() {
                if !self.data.is_empty() {
                    events.push(self.data.join("\n"));
                    self.data.clear();
                }
            } else if let Some(data) = line.strip_prefix("data:") {
                self.data
                    .push(data.strip_prefix(' ').unwrap_or(data).into());
                if self.data.iter().map(String::len).sum::<usize>() > 4 * 1024 * 1024 {
                    return Err(AppError::Model("流式事件过大".into()));
                }
            }
        }
        Ok(events)
    }
}

// Extract only user-facing string values from an incomplete JSON report. Escape
// sequences and split unicode escapes are decoded before showing a draft.
pub(crate) fn draft_report(raw: &str) -> String {
    let mut result = String::new();
    let mut key = String::new();
    let mut chars = raw.char_indices().peekable();
    while let Some((start, ch)) = chars.next() {
        if ch != '"' {
            continue;
        }
        let mut end = raw.len();
        let mut closed = false;
        while let Some((i, c)) = chars.next() {
            if c == '\\' {
                chars.next();
            } else if c == '"' {
                end = i;
                closed = true;
                break;
            }
        }
        let encoded = &raw[start + 1..end];
        let mut value = None;
        // Remove an unfinished escape, including a split UTF-16 surrogate pair.
        for cut in (encoded.len().saturating_sub(12)..=encoded.len())
            .rev()
            .filter(|i| encoded.is_char_boundary(*i))
        {
            if let Ok(decoded) = serde_json::from_str::<String>(&format!("\"{}\"", &encoded[..cut]))
            {
                value = Some(decoded);
                break;
            }
        }
        let value = value.unwrap_or_default();
        let is_key = closed && raw[end + 1..].trim_start().starts_with(':');
        if is_key {
            key = value;
            let title = match key.as_str() {
                "suggestions" => Some("建议"),
                "question" => Some("追问"),
                _ => None,
            };
            if let Some(title) = title {
                if !result.is_empty() {
                    result.push_str("\n\n");
                }
                result.push_str(title);
            }
        } else if matches!(
            key.as_str(),
            "answer" | "action" | "why" | "unlessWhen" | "question"
        ) && !value.is_empty()
        {
            if !result.is_empty() {
                result.push('\n');
            }
            result.push_str(&value);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_fragmented_utf8_crlf_multiline_and_comments() {
        let mut decoder = SseDecoder::default();
        let bytes = ": ping\r\n\r\ndata: 中文\r\ndata: two\r\n\r\n".as_bytes();
        let mut events = Vec::new();
        for byte in bytes {
            events.extend(decoder.feed(&[*byte]).unwrap());
        }
        assert_eq!(events, vec!["中文\ntwo"]);
    }
    #[test]
    fn projects_partial_strings_without_references_or_json_syntax() {
        assert_eq!(draft_report(r#"{"answer":"先\n核实\u4e2d\u"#), "先\n核实中");
        assert_eq!(
            draft_report(
                r#"{"answer":"结论","facts":[{"statement":"隐藏事实","sourceRefs":["/private/path"]}],"suggestions":[{"action":"减少集中持仓"#
            ),
            "结论\n\n建议\n减少集中持仓"
        );
    }
}
