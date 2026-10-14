//! Streaming wire parser and draft projection. Drafts never bypass report validation.
use crate::{AppError, AppResult};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(crate) enum AnalysisEvent {
    Status {
        message: String,
    },
    Draft {
        answer: String,
    },
    Complete {
        result: Box<crate::models::AnalysisResult>,
    },
    Error {
        message: String,
    },
}
pub(crate) type EventSender = tokio::sync::mpsc::Sender<AnalysisEvent>;
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
                "verdict" => Some("当前判断"),
                "facts" => Some("已知事实"),
                "inferences" => Some("合理推断"),
                "unknowns" => Some("仍待核实"),
                "options" => Some("方案比较"),
                "actions" => Some("下一步行动"),
                "reviewTriggers" => Some("复盘与证伪条件"),
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
            "verdict"
                | "statement"
                | "unknowns"
                | "name"
                | "suitableWhen"
                | "tradeoffs"
                | "risks"
                | "action"
                | "rationale"
                | "evidenceLimits"
                | "reviewTrigger"
                | "reviewTriggers"
        ) && !value.is_empty()
        {
            result.push('\n');
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
        assert_eq!(
            draft_report(r#"{"verdict":"先\n核实\u4e2d\u"#),
            "当前判断\n先\n核实中"
        );
        assert_eq!(
            draft_report(
                r#"{"facts":[{"statement":"已知事实","sourceRefs":["/private/path"]}],"unknowns":["待核"#
            ),
            "已知事实\n已知事实\n\n仍待核实\n待核"
        );
    }
}
