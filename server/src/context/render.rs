//! Model-facing Markdown. Internal JSON remains available for citations and audit only.
use serde_json::Value;
fn cell(value: &Value) -> Option<String> {
    let text = match value {
        Value::String(s) if !s.trim().is_empty() => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(v) => v.to_string(),
        _ => return None,
    };
    Some(
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('|', "&#124;")
            .replace(['\r', '\n'], "<br>"),
    )
}
fn segment(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}
fn block(value: &Value) -> bool {
    matches!(value, Value::String(text) if text.len()>160 || text.contains(['&','<','>','|','\n','\r']))
}
pub fn render_for_model(value: &Value) -> String {
    fn render(value: &Value, path: &str, out: &mut String) {
        match value {
            Value::Object(map) if path == "/facts/holdings" && !map.is_empty() => {
                let fields = [
                    "name",
                    "symbol",
                    "currency",
                    "currentQuantity",
                    "quantityUnit",
                    "contract",
                    "costBasis",
                    "marketValue",
                    "unrealizedProfit",
                    "weightPct",
                ];
                let columns = fields
                    .into_iter()
                    .filter(|key| map.values().any(|v| cell(&v[*key]).is_some()))
                    .collect::<Vec<_>>();
                out.push_str("\n## 当前持仓\n引用格式：/facts/holdings/{行ID}/{列名}\n| 行ID |");
                for key in &columns {
                    out.push_str(&format!(" {key} |"));
                }
                out.push_str("\n| --- |");
                for _ in &columns {
                    out.push_str(" --- |");
                }
                out.push('\n');
                for (id, row) in map {
                    out.push_str(&format!(
                        "| {} |",
                        cell(&Value::String(segment(id))).unwrap_or_default()
                    ));
                    for key in &columns {
                        out.push_str(&format!(" {} |", cell(&row[*key]).unwrap_or_default()));
                    }
                    out.push('\n');
                }
            }
            Value::Object(map) => {
                let scalars = map
                    .iter()
                    .filter_map(|(key, value)| {
                        (!block(value))
                            .then(|| cell(value))
                            .flatten()
                            .map(|value| (key, value))
                    })
                    .collect::<Vec<_>>();
                if !scalars.is_empty() {
                    out.push_str(&format!(
                        "\n### {}\n| 字段 | 值 |\n| --- | --- |\n",
                        if path.is_empty() {
                            "本轮输入"
                        } else {
                            path
                        }
                    ));
                    for (key, value) in scalars {
                        out.push_str(&format!("| {} | {value} |\n", segment(key)));
                    }
                }
                for (key, value) in map {
                    if cell(value).is_none() || block(value) {
                        render(value, &format!("{path}/{}", segment(key)), out);
                    }
                }
            }
            Value::Array(values) => {
                for (index, value) in values.iter().enumerate() {
                    render(value, &format!("{path}/{index}"), out);
                }
            }
            _ if block(value) => {
                let text = value.as_str().unwrap_or_default();
                let fence = "`".repeat(
                    text.split(|c| c != '`')
                        .map(str::len)
                        .max()
                        .unwrap_or(0)
                        .max(2)
                        + 1,
                );
                out.push_str(&format!("\n### {path}\n{fence}text\n{text}\n{fence}\n"));
            }
            _ => {
                if let Some(value) = cell(value) {
                    out.push_str(&format!("- {path}: {value}\n"));
                }
            }
        }
    }
    let mut output = String::new();
    render(value, "", &mut output);
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn current_states_are_tables_and_long_originals_keep_exact_text_without_json() {
        let text = "原始问题含 A&B <条件> | 表格符号\n```原文围栏```";
        let payload = serde_json::json!({"question":text,"facts":{"summary":{"monthlySurplus":"0"},"holdings":{"h":{"name":"测试标的","currency":"CNY","marketValue":"900719925474099.91"}}},"researchEvidence":[{"id":"e1","claim":"原文"}]});
        let rendered = render_for_model(&payload);
        assert!(rendered.contains(text));
        assert!(rendered.contains("````text"));
        assert!(rendered.contains("| 行ID | name | currency | marketValue |"));
        assert!(rendered.contains("900719925474099.91"));
        assert!(rendered.contains("| monthlySurplus | 0 |"));
        assert!(!rendered.contains("\"marketValue\""));
        assert!(!rendered.contains("null"));
    }
}
