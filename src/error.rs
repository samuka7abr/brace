//! Erros de sintaxe produzidos pelo lexer e pelo parser.

/// Erro de sintaxe encontrado ao parsear um texto JSON.
///
/// Carrega, além do tipo do problema (`kind`), a posição exata (`line`,
/// `col`) onde ele foi detectado — coluna e linha contam caracteres
/// Unicode (scalar values), não bytes.
#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub kind: ParseErrorKind,
    pub line: usize,
    pub col: usize,
}

impl ParseError {
    /// Cria um novo `ParseError` com a posição informada.
    pub(crate) fn new(kind: ParseErrorKind, line: usize, col: usize) -> Self {
        ParseError { kind, line, col }
    }
}

/// Categoria do erro de sintaxe.
///
/// `UnexpectedToken.found` é `&'static str` em vez de um tipo `Token`
/// interno: `Token` é privado ao crate e `ParseErrorKind` é público, então
/// um campo público não pode expor um tipo privado. O `&'static str`
/// carrega a descrição do token encontrado (por exemplo `"}"`, `"string"`,
/// `"número"`, `"fim da entrada"`).
#[derive(Debug, Clone, PartialEq)]
pub enum ParseErrorKind {
    /// Caractere que não inicia nenhum token válido.
    UnexpectedChar(char),
    /// Token esperado pela gramática não é o token encontrado.
    UnexpectedToken {
        expected: &'static str,
        found: &'static str,
    },
    /// String iniciada com `"` mas nunca fechada.
    UnterminatedString,
    /// Sequência de escape (`\X`) com `X` que não é um escape válido.
    InvalidEscape(char),
    /// Número que não segue a gramática de `number` da RFC 8259.
    InvalidNumber,
    /// Entrada terminou onde um token era esperado.
    UnexpectedEof,
    /// `\uXXXX` com par substituto UTF-16 solto ou incompleto.
    InvalidUnicodeEscape,
    /// Profundidade de aninhamento de objetos/arrays acima do limite permitido.
    DepthLimitExceeded,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "erro de sintaxe na linha {}, coluna {}: {}",
            self.line, self.col, self.kind
        )
    }
}

impl std::fmt::Display for ParseErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseErrorKind::UnexpectedChar(c) => write!(f, "caractere inesperado \"{c}\""),
            ParseErrorKind::UnexpectedToken { expected, found } => {
                write!(f, "esperava {expected}, encontrou \"{found}\"")
            }
            ParseErrorKind::UnterminatedString => write!(f, "string não terminada"),
            ParseErrorKind::InvalidEscape(c) => write!(f, "escape inválido \"\\{c}\""),
            ParseErrorKind::InvalidNumber => write!(f, "número inválido"),
            ParseErrorKind::UnexpectedEof => write!(f, "fim de entrada inesperado"),
            ParseErrorKind::InvalidUnicodeEscape => {
                write!(f, "escape unicode (\\uXXXX) inválido")
            }
            ParseErrorKind::DepthLimitExceeded => {
                write!(f, "limite de profundidade de aninhamento excedido")
            }
        }
    }
}

impl std::error::Error for ParseError {}

#[cfg(test)]
mod tests {
    use super::{ParseError, ParseErrorKind};

    #[test]
    fn display_inclui_linha_coluna_e_mensagem() {
        let erro = ParseError::new(
            ParseErrorKind::UnexpectedToken {
                expected: "valor",
                found: ",",
            },
            2,
            8,
        );

        assert_eq!(
            erro.to_string(),
            "erro de sintaxe na linha 2, coluna 8: esperava valor, encontrou \",\""
        );
    }

    #[test]
    fn campos_do_parse_error_sao_acessiveis() {
        let erro = ParseError::new(ParseErrorKind::UnexpectedEof, 1, 1);
        assert_eq!(erro.line, 1);
        assert_eq!(erro.col, 1);
        assert_eq!(erro.kind, ParseErrorKind::UnexpectedEof);
    }
}
