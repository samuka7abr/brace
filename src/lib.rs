//! `brace` é um parser de JSON (RFC 8259) escrito do zero, sem crates de
//! parsing externas.
//!
//! A API pública é só [`parse`], [`Value`], [`ParseError`] e
//! [`ParseErrorKind`] — lexer e parser ficam privados ao crate.
//!
//! # Limitações conhecidas
//!
//! `Value::Number` é `f64` (perde precisão acima de 2^53 e não distingue
//! `1` de `1.0`) e `Value::Object` é `Vec<(String, Value)>` (preserva
//! ordem das chaves, mas lookup é O(n) e chaves duplicadas não sofrem
//! dedup). Ver a documentação de [`Value`] para detalhes.

mod error;
mod lexer;
mod parser;
mod value;

pub use error::{ParseError, ParseErrorKind};
pub use value::Value;

/// Parseia uma string de entrada como um documento JSON completo.
///
/// # Exemplos
///
/// ```rust
/// use brace::Value;
///
/// let valor = brace::parse(r#"{"nome": "brace", "versao": 1}"#).unwrap();
///
/// match valor {
///     Value::Object(pares) => assert_eq!(pares.len(), 2),
///     _ => unreachable!(),
/// }
/// ```
///
/// # Erros
///
/// Retorna `Err(ParseError)` se a entrada não for um JSON sintaticamente
/// válido. O erro carrega `line` e `col` apontando para o caractere ou
/// token onde o problema foi detectado, além do [`ParseErrorKind`] com o
/// tipo do problema.
pub fn parse(input: &str) -> Result<Value, ParseError> {
    let tokens = lexer::tokenize(input)?;
    parser::parse_tokens(&tokens)
}
