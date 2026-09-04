use std::{
    fs,
    io::{self, BufRead, Read, Write},
    path::PathBuf,
};

fn main() {
    let mut args = std::env::args();
    let _executable = args.next().unwrap();
    assert_eq!(args.next().as_deref(), Some("acp"));
    assert_eq!(args.next().as_deref(), Some("-C"));
    let root = PathBuf::from(args.next().unwrap());
    assert!(args.next().is_none());
    let scenario = fs::read_to_string(root.join("scenario.json")).unwrap();
    if scenario.contains("early-eof") {
        return;
    }
    let stdin = io::stdin();
    let mut line = String::new();
    stdin.lock().read_line(&mut line).unwrap();
    let request: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(request["method"], "initialize");
    assert_eq!(request["params"]["protocolVersion"], 1);
    let id = request["id"].clone();
    let response = match scenario.trim() {
        r#"{"protocol":"ready"}"# => {
            serde_json::json!({"jsonrpc":"2.0","id":id,"result":{"protocolVersion":1,"agentInfo":{"name":"ego","version":"test"},"agentCapabilities":{}}}).to_string()
        }
        r#"{"protocol":"ready-eof"}"# => {
            serde_json::json!({"jsonrpc":"2.0","id":id,"result":{"protocolVersion":1,"agentInfo":{"name":"ego","version":"test"},"agentCapabilities":{}}}).to_string()
        }
        r#"{"protocol":"ready-alive"}"# => {
            serde_json::json!({"jsonrpc":"2.0","id":id,"result":{"protocolVersion":1,"agentInfo":{"name":"ego","version":"test"},"agentCapabilities":{}}}).to_string()
        }
        r#"{"protocol":"non-v1"}"# => {
            serde_json::json!({"jsonrpc":"2.0","id":id,"result":{"protocolVersion":2,"agentInfo":{"name":"ego","version":"test"},"agentCapabilities":{}}}).to_string()
        }
        _ => "not-json".to_string(),
    };
    println!("{response}");
    io::stdout().flush().unwrap();
    if scenario.trim() == r#"{"protocol":"ready-alive"}"# {
        let mut closed = Vec::new();
        let _ = io::stdin().read_to_end(&mut closed);
    }
}
