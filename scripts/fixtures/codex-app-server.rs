//! Disposable protocol fixture. Never reads Codex credentials or calls a model.
use std::{
    env, fs,
    io::{self, BufRead, Write},
};
fn emit(value: String) {
    println!("{value}");
    io::stdout().flush().unwrap();
}
fn quote(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
    )
}
fn main() {
    let scenario = env::var("MARIO_TEST_CODEX_SCENARIO").unwrap();
    let counter = env::var("MARIO_TEST_CODEX_COUNTER").unwrap();
    let report = fs::read_to_string(env::var("MARIO_TEST_CODEX_REPORT").unwrap()).unwrap();
    for line in io::stdin().lock().lines() {
        let line = line.unwrap();
        let Some(rest) = line.split("\"id\":").nth(1) else {
            continue;
        };
        let id = rest
            .chars()
            .take_while(|ch| ch.is_ascii_digit())
            .collect::<String>();
        if line.contains("\"method\":\"initialize\"") {
            emit(format!(r#"{{"id":{id},"result":{{}}}}"#));
        } else if line.contains("\"method\":\"account/read\"") {
            emit(format!(
                r#"{{"id":{id},"result":{{"account":{{"type":"chatgpt"}}}}}}"#
            ));
        } else if line.contains("\"method\":\"model/list\"") {
            emit(format!(
                r#"{{"id":{id},"result":{{"data":[{{"model":"bridge-fixture","displayName":"Default","isDefault":true}},{{"model":"bridge-alternative","displayName":"Alternative","isDefault":false}}],"nextCursor":null}}}}"#
            ));
        } else if line.contains("\"method\":\"thread/start\"") {
            assert!(line.contains("\"sandbox\":\"read-only\""));
            assert!(line.contains("\"ephemeral\":true"));
            assert!(line.contains("\"model\":\"bridge-alternative\""));
            emit(format!(
                r#"{{"id":{id},"result":{{"thread":{{"id":"fixture-thread"}}}}}}"#
            ));
        } else if line.contains("\"method\":\"turn/start\"") {
            let count = fs::read_to_string(&counter)
                .unwrap_or_default()
                .parse::<usize>()
                .unwrap_or(0)
                + 1;
            fs::write(&counter, count.to_string()).unwrap();
            emit(format!(r#"{{"id":{id},"result":{{}}}}"#));
            let mode = fs::read_to_string(&scenario).unwrap();
            if mode == "partial" {
                emit(
                    r#"{"method":"item/agentMessage/delta","params":{"delta":"partial-answer"}}"#
                        .into(),
                );
            }
            let retry = mode == "native-retry";
            if mode == "quota" || mode == "partial" || retry || (mode == "transient" && count == 1)
            {
                let info = if mode == "quota" {
                    r#""usageLimitExceeded""#
                } else {
                    r#"{"httpConnectionFailed":{"httpStatusCode":503}}"#
                };
                emit(format!(
                    r#"{{"method":"error","params":{{"error":{{"message":"SECRET-test-provider-body","codexErrorInfo":{info}}},"willRetry":{retry}}}}}"#
                ));
                if !retry {
                    continue;
                }
            }
            emit(r#"{"method":"item/completed","params":{"item":{"type":"webSearch","results":[]}}}"#.into());
            emit(format!(
                r#"{{"method":"item/agentMessage/delta","params":{{"delta":{}}}}}"#,
                quote(&report)
            ));
            emit(format!(
                r#"{{"method":"item/completed","params":{{"item":{{"type":"agentMessage","text":{}}}}}}}"#,
                quote(&report)
            ));
            emit(r#"{"method":"turn/completed","params":{"turn":{"status":"completed"}}}"#.into());
        }
    }
}
