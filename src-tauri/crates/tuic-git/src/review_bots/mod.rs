//! Review-bot domain shared by every forge adapter: who wrote a review comment.

use serde_json::Value;

/// A GraphQL `author` (or REST `user`) object is a bot when the forge says so:
/// GraphQL `__typename` "Bot", REST `type` "Bot". The `[bot]` login suffix is
/// only a fallback, because GraphQL bot logins (`cursor`, `coderabbitai`) and
/// the REST login of Copilot line comments (`Copilot`) carry no suffix.
pub fn is_bot(author: &Value) -> bool {
    author["__typename"].as_str() == Some("Bot")
        || author["type"].as_str() == Some("Bot")
        || author["login"]
            .as_str()
            .is_some_and(|login| login.ends_with("[bot]"))
}
