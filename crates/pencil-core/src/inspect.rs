//! How text reads, stage by stage, for a page that shows it: the tokens the lexer makes, then
//! the expression it is, or else the document's statements and the IR they compile to. A stage
//! that fails gives its error, with where it is, in place of what it would have made.

use serde_json::{Value, json};

use crate::{Error, compile, parse};

fn error(e: Error) -> Value {
    json!({ "error": e.message, "line": e.line, "col": e.col })
}

pub fn inspect(src: &str) -> Value {
    let tokens: Value = match parse::tokens(src) {
        Ok(tokens) => tokens
            .iter()
            .map(|t| json!({ "kind": t.kind, "text": t.text, "line": t.line, "col": t.col }))
            .collect::<Vec<_>>()
            .into(),
        Err(e) => return json!({ "tokens": error(e) }),
    };
    let expression = match parse::parse_whole_expression(src) {
        Ok(e) => return json!({ "tokens": tokens, "expression": e.to_mathjson() }),
        Err(e) => error(e),
    };
    let document = match parse::parse(src) {
        Ok(document) => document,
        Err(e) => {
            return json!({ "tokens": tokens, "expression": expression, "document": error(e) });
        }
    };
    let statements: Vec<String> = document.items.iter().map(|i| format!("{i:#?}")).collect();
    let ir = match compile::compile(&document) {
        Ok(scene) => scene.to_json(),
        Err(e) => error(e),
    };
    json!({ "tokens": tokens, "document": statements, "ir": ir })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_expression_shows_its_tokens_and_its_tree() {
        let seen = inspect("2·π + x²");
        let texts: Vec<&str> = seen["tokens"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["text"].as_str().unwrap())
            .collect();
        assert_eq!(texts, ["2", "*", "π", "+", "x", "^2"]);
        assert_eq!(
            seen["expression"],
            json!(["Add", ["Multiply", 2.0, "π"], ["Power", "x", 2.0]])
        );
    }

    #[test]
    fn a_document_shows_its_statements_and_ir() {
        let seen = inspect("param a in 0..1\nlet b = 2·a");
        assert_eq!(seen["document"].as_array().unwrap().len(), 2);
        assert!(seen["ir"]["bindings"].is_array(), "{}", seen["ir"]);
    }

    #[test]
    fn what_reads_as_neither_says_where_it_stopped() {
        let seen = inspect("2π");
        assert_eq!(seen["expression"]["col"], 2, "{seen}");
        assert!(seen["document"]["error"].is_string(), "{seen}");
    }
}
