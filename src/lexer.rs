//! Lexer do `brace`.
//!
//! Varre a entrada `&str` caractere a caractere e produz uma sequência de
//! [`Token`]s. Não conhece gramática (não sabe que um objeto tem chaves
//! seguidas de `:` e valores) — só reconhece unidades léxicas isoladas:
//! estruturais (`{ } [ ] : ,`), literais (`true`/`false`/`null`), strings e
//! números. A decisão de como esses tokens se combinam é do parser.

use crate::error::{ParseError, ParseErrorKind};

/// Tipo de um token, sem posição. Ver [`Token`] para a versão posicionada.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TokenKind {
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Colon,
    Comma,
    String(String),
    Number(f64),
    True,
    False,
    Null,
    Eof,
}

impl TokenKind {
    /// Descrição curta do token, usada nas mensagens de erro do parser
    /// (ex.: "esperava `:`, encontrou `,`").
    pub(crate) fn describe(&self) -> &'static str {
        match self {
            TokenKind::LBrace => "{",
            TokenKind::RBrace => "}",
            TokenKind::LBracket => "[",
            TokenKind::RBracket => "]",
            TokenKind::Colon => ":",
            TokenKind::Comma => ",",
            TokenKind::String(_) => "string",
            TokenKind::Number(_) => "number",
            TokenKind::True => "true",
            TokenKind::False => "false",
            TokenKind::Null => "null",
            TokenKind::Eof => "fim da entrada",
        }
    }
}

/// Um token com a posição (linha/coluna, ambas contadas a partir de 1) do
/// seu PRIMEIRO caractere. É essa posição que o parser copia para dentro de
/// um `ParseError` ao encontrar um token inesperado — o parser nunca precisa
/// manter estado de posição próprio.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Token {
    pub(crate) kind: TokenKind,
    pub(crate) line: usize,
    pub(crate) col: usize,
}

/// Tokeniza a entrada inteira, retornando o vetor completo de tokens
/// (sempre terminado por um [`TokenKind::Eof`]) ou o primeiro erro léxico
/// encontrado.
pub(crate) fn tokenize(input: &str) -> Result<Vec<Token>, ParseError> {
    let mut lexer = Lexer::new(input);
    let mut tokens = Vec::new();

    loop {
        lexer.skip_whitespace();
        let line = lexer.line;
        let col = lexer.col;

        let Some(c) = lexer.peek() else {
            tokens.push(Token {
                kind: TokenKind::Eof,
                line,
                col,
            });
            break;
        };

        let kind = match c {
            '{' => {
                lexer.advance();
                TokenKind::LBrace
            }
            '}' => {
                lexer.advance();
                TokenKind::RBrace
            }
            '[' => {
                lexer.advance();
                TokenKind::LBracket
            }
            ']' => {
                lexer.advance();
                TokenKind::RBracket
            }
            ':' => {
                lexer.advance();
                TokenKind::Colon
            }
            ',' => {
                lexer.advance();
                TokenKind::Comma
            }
            '"' => lexer.lex_string()?,
            // '+' e '.' não começam nenhum token válido em JSON, mas quem
            // escreveu ".5" ou "+1" quase certamente tentou escrever um
            // número — delegamos ao lexer de números, que aplica a
            // gramática estrita e devolve `InvalidNumber` num caso desses,
            // em vez de um `UnexpectedChar` pouco informativo.
            '-' | '+' | '.' | '0'..='9' => lexer.lex_number()?,
            't' => lexer.lex_keyword("true", TokenKind::True)?,
            'f' => lexer.lex_keyword("false", TokenKind::False)?,
            'n' => lexer.lex_keyword("null", TokenKind::Null)?,
            _ => {
                return Err(ParseError::new(
                    ParseErrorKind::UnexpectedChar(c),
                    line,
                    col,
                ));
            }
        };

        tokens.push(Token { kind, line, col });
    }

    Ok(tokens)
}

/// Cursor interno sobre a entrada. Não é exposto fora deste módulo.
struct Lexer<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
    line: usize,
    col: usize,
}

impl<'a> Lexer<'a> {
    fn new(input: &'a str) -> Self {
        Lexer {
            chars: input.chars().peekable(),
            line: 1,
            col: 1,
        }
    }

    /// Único ponto do lexer que consome um caractere da entrada e avança o
    /// cursor de linha/coluna. Todo outro método consome caracteres através
    /// deste, nunca diretamente do iterador — é isso que garante que
    /// linha/coluna fiquem corretas em qualquer caminho de código.
    ///
    /// Itera por `char` (Unicode scalar value), não por byte: um caractere
    /// UTF-8 multi-byte conta como uma única coluna.
    fn advance(&mut self) -> Option<char> {
        let c = self.chars.next();
        if let Some(ch) = c {
            if ch == '\n' {
                self.line += 1;
                self.col = 1;
            } else {
                self.col += 1;
            }
        }
        c
    }

    /// Olha o próximo caractere sem consumi-lo.
    fn peek(&mut self) -> Option<char> {
        self.chars.peek().copied()
    }

    /// Consome um caractere e o empilha em `buf`, se houver. Usado pelo
    /// lexer de números para ir montando a substring validada sem nunca
    /// arriscar um `unwrap` sobre um `peek` que já garantimos existir.
    fn bump_into(&mut self, buf: &mut String) {
        if let Some(c) = self.advance() {
            buf.push(c);
        }
    }

    /// Consome espaço em branco. RFC 8259 define só estes quatro
    /// caracteres como espaço em branco significativo entre tokens —
    /// qualquer outro caractere fora de um valor é erro (`UnexpectedChar`),
    /// tratado naturalmente pelo chamador quando este método para de
    /// consumir e o caractere restante não casa com nenhum token.
    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek() {
            match c {
                ' ' | '\t' | '\n' | '\r' => {
                    self.advance();
                }
                _ => break,
            }
        }
    }

    /// Casa uma palavra reservada (`true`, `false`, `null`) por completo.
    /// Compara caractere a caractere contra a palavra esperada: um prefixo
    /// (`tru`) ou uma divergência no meio nunca é aceito como a palavra —
    /// gera erro no primeiro caractere que não bate (ou `UnexpectedEof` se
    /// a entrada terminar antes da palavra completa).
    fn lex_keyword(&mut self, word: &str, kind: TokenKind) -> Result<TokenKind, ParseError> {
        for expected in word.chars() {
            let line = self.line;
            let col = self.col;
            match self.advance() {
                Some(c) if c == expected => {}
                Some(c) => {
                    return Err(ParseError::new(
                        ParseErrorKind::UnexpectedChar(c),
                        line,
                        col,
                    ));
                }
                None => {
                    return Err(ParseError::new(ParseErrorKind::UnexpectedEof, line, col));
                }
            }
        }
        Ok(kind)
    }

    /// Lexa uma string JSON. Chamado com a aspa de abertura ainda não
    /// consumida (só espiada pelo laço principal); consome-a aqui.
    ///
    /// Caracteres de controle literais (`U+0000`–`U+001F`) dentro da string
    /// são proibidos pelo RFC e geram `UnexpectedChar`, não
    /// `UnterminatedString` — só a ausência de aspa de fechamento (incluindo
    /// EOF no meio da string ou no meio de um escape) é `UnterminatedString`.
    fn lex_string(&mut self) -> Result<TokenKind, ParseError> {
        let start_line = self.line;
        let start_col = self.col;
        self.advance(); // consome a aspa de abertura

        let mut value = String::new();

        loop {
            let char_line = self.line;
            let char_col = self.col;
            match self.advance() {
                None => {
                    return Err(ParseError::new(
                        ParseErrorKind::UnterminatedString,
                        start_line,
                        start_col,
                    ));
                }
                Some('"') => break,
                Some('\\') => {
                    let esc_line = self.line;
                    let esc_col = self.col;
                    match self.advance() {
                        None => {
                            return Err(ParseError::new(
                                ParseErrorKind::UnterminatedString,
                                start_line,
                                start_col,
                            ));
                        }
                        Some('"') => value.push('"'),
                        Some('\\') => value.push('\\'),
                        Some('/') => value.push('/'),
                        Some('b') => value.push('\u{0008}'),
                        Some('f') => value.push('\u{000C}'),
                        Some('n') => value.push('\n'),
                        Some('r') => value.push('\r'),
                        Some('t') => value.push('\t'),
                        Some('u') => {
                            let c = self.lex_unicode_escape()?;
                            value.push(c);
                        }
                        Some(other) => {
                            return Err(ParseError::new(
                                ParseErrorKind::InvalidEscape(other),
                                esc_line,
                                esc_col,
                            ));
                        }
                    }
                }
                Some(c) if (c as u32) < 0x20 => {
                    return Err(ParseError::new(
                        ParseErrorKind::UnexpectedChar(c),
                        char_line,
                        char_col,
                    ));
                }
                Some(c) => value.push(c),
            }
        }

        Ok(TokenKind::String(value))
    }

    /// Lê exatamente 4 dígitos hexadecimais (uma *code unit* UTF-16) depois
    /// de `\u`. Aceita hex maiúsculo ou minúsculo. Qualquer coisa que não
    /// seja um dígito hexadecimal — incluindo a entrada terminar antes dos
    /// 4 dígitos — é `InvalidUnicodeEscape`.
    fn read_hex4(&mut self) -> Result<u16, ParseError> {
        let line = self.line;
        let col = self.col;
        let mut value: u32 = 0;
        for _ in 0..4 {
            let digit = self.advance().and_then(|c| c.to_digit(16));
            match digit {
                Some(d) => value = value * 16 + d,
                None => {
                    return Err(ParseError::new(
                        ParseErrorKind::InvalidUnicodeEscape,
                        line,
                        col,
                    ));
                }
            }
        }
        Ok(value as u16)
    }

    /// Ponto mais escorregadio do lexer: resolve um escape `\uXXXX`,
    /// incluindo o caso de pares substitutos UTF-16.
    ///
    /// A code unit lida por [`read_hex4`](Self::read_hex4) pode ser:
    ///
    /// - Fora das faixas substitutas (`0xD800..=0xDFFF`): é diretamente um
    ///   code point, convertido com `char::from_u32`.
    /// - Uma substituta **alta** (`0xD800..=0xDBFF`): precisa ser seguida
    ///   IMEDIATAMENTE por outro escape `\u` com uma substituta **baixa**
    ///   (`0xDC00..=0xDFFF`). O par é combinado em
    ///   `0x10000 + (alta - 0xD800) * 0x400 + (baixa - 0xDC00)`. Se o que
    ///   vier depois não for exatamente isso — outro caractere, outra
    ///   substituta alta, ou a entrada acabar — é `InvalidUnicodeEscape`.
    ///   Não há fallback silencioso para `U+FFFD`: um par incompleto é
    ///   entrada inválida, ponto.
    /// - Uma substituta **baixa** desacompanhada (sem uma alta antes): pelo
    ///   mesmo motivo, também é `InvalidUnicodeEscape`.
    fn lex_unicode_escape(&mut self) -> Result<char, ParseError> {
        let start_line = self.line;
        let start_col = self.col;
        let unit = self.read_hex4()?;

        if (0xD800..=0xDBFF).contains(&unit) {
            // Substituta alta: o próximo escape TEM que ser "\u" seguido de
            // uma substituta baixa. Qualquer outra coisa é par incompleto.
            match self.advance() {
                Some('\\') => {}
                _ => {
                    return Err(ParseError::new(
                        ParseErrorKind::InvalidUnicodeEscape,
                        start_line,
                        start_col,
                    ));
                }
            }
            match self.advance() {
                Some('u') => {}
                _ => {
                    return Err(ParseError::new(
                        ParseErrorKind::InvalidUnicodeEscape,
                        start_line,
                        start_col,
                    ));
                }
            }
            let low = self.read_hex4()?;
            if !(0xDC00..=0xDFFF).contains(&low) {
                // Veio outra substituta alta, ou uma code unit qualquer que
                // não é baixa: par incompleto.
                return Err(ParseError::new(
                    ParseErrorKind::InvalidUnicodeEscape,
                    start_line,
                    start_col,
                ));
            }
            let high_bits = u32::from(unit) - 0xD800;
            let low_bits = u32::from(low) - 0xDC00;
            let codepoint = 0x10000 + high_bits * 0x400 + low_bits;
            char::from_u32(codepoint).ok_or_else(|| {
                ParseError::new(ParseErrorKind::InvalidUnicodeEscape, start_line, start_col)
            })
        } else if (0xDC00..=0xDFFF).contains(&unit) {
            // Substituta baixa sem uma alta antes.
            Err(ParseError::new(
                ParseErrorKind::InvalidUnicodeEscape,
                start_line,
                start_col,
            ))
        } else {
            char::from_u32(u32::from(unit)).ok_or_else(|| {
                ParseError::new(ParseErrorKind::InvalidUnicodeEscape, start_line, start_col)
            })
        }
    }

    /// Lexa um número seguindo estritamente a gramática do RFC 8259:
    ///
    /// ```text
    /// "-"?  ("0" | [1-9][0-9]*)  ("." [0-9]+)?  ([eE] [+-]? [0-9]+)?
    /// ```
    ///
    /// A validação é feita caractere a caractere — a decisão de "isso é um
    /// número válido?" nunca é delegada ao `str::parse`. Só depois de
    /// reconhecer a forma exata (delimitando a substring em `buf`) é que
    /// ela é convertida com `str::parse::<f64>()`; nesse ponto o parse não
    /// deveria falhar (a gramática já garantiu uma forma numérica válida),
    /// mas se falhar por algum motivo isso é mapeado para `InvalidNumber`
    /// em vez de um `unwrap`.
    ///
    /// Zero à esquerda (`01`) é proibido — `0`, `0.5` e `-0` são válidos.
    /// Um número sintaticamente válido mas fora do alcance de `f64` (ex.:
    /// `1e400`) é aceito de propósito e resulta em `f64::INFINITY`, não em
    /// erro: a gramática não proíbe isso, e RFC 8259 permite que
    /// implementações imponham (ou não) limites próprios de alcance.
    fn lex_number(&mut self) -> Result<TokenKind, ParseError> {
        let start_line = self.line;
        let start_col = self.col;
        let mut buf = String::new();

        if self.peek() == Some('-') {
            self.bump_into(&mut buf);
        }

        match self.peek() {
            Some('0') => {
                self.bump_into(&mut buf);
                // "0" seguido de outro dígito (ex.: "01") é zero à esquerda,
                // proibido pelo RFC.
                if matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                    return Err(ParseError::new(
                        ParseErrorKind::InvalidNumber,
                        start_line,
                        start_col,
                    ));
                }
            }
            Some(c) if c.is_ascii_digit() => {
                self.bump_into(&mut buf);
                while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                    self.bump_into(&mut buf);
                }
            }
            _ => {
                // Nem "-" sozinho, nem "+1"/".5" (dígito inicial ausente)
                // formam a parte inteira exigida pela gramática.
                return Err(ParseError::new(
                    ParseErrorKind::InvalidNumber,
                    start_line,
                    start_col,
                ));
            }
        }

        if self.peek() == Some('.') {
            self.bump_into(&mut buf);
            let mut has_digit = false;
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                self.bump_into(&mut buf);
                has_digit = true;
            }
            if !has_digit {
                // "1." sem dígito depois do ponto.
                return Err(ParseError::new(
                    ParseErrorKind::InvalidNumber,
                    start_line,
                    start_col,
                ));
            }
        }

        if matches!(self.peek(), Some('e') | Some('E')) {
            self.bump_into(&mut buf);
            if matches!(self.peek(), Some('+') | Some('-')) {
                self.bump_into(&mut buf);
            }
            let mut has_digit = false;
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                self.bump_into(&mut buf);
                has_digit = true;
            }
            if !has_digit {
                // "1e" ou "1e+" sem dígito depois do expoente.
                return Err(ParseError::new(
                    ParseErrorKind::InvalidNumber,
                    start_line,
                    start_col,
                ));
            }
        }

        let value: f64 = buf
            .parse()
            .map_err(|_| ParseError::new(ParseErrorKind::InvalidNumber, start_line, start_col))?;

        Ok(TokenKind::Number(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: tokeniza e retorna só os `TokenKind`, descartando posição,
    /// para os testes que só se importam com a sequência de tokens.
    fn kinds(input: &str) -> Vec<TokenKind> {
        tokenize(input)
            .expect("esperava tokenização bem-sucedida")
            .into_iter()
            .map(|t| t.kind)
            .collect()
    }

    fn err(input: &str) -> ParseError {
        tokenize(input).expect_err("esperava erro de tokenização")
    }

    // --- estruturais e literais ---------------------------------------

    #[test]
    fn empty_braces() {
        assert_eq!(
            kinds("{ }"),
            vec![TokenKind::LBrace, TokenKind::RBrace, TokenKind::Eof]
        );
    }

    #[test]
    fn array_of_literals() {
        assert_eq!(
            kinds("[true, false, null]"),
            vec![
                TokenKind::LBracket,
                TokenKind::True,
                TokenKind::Comma,
                TokenKind::False,
                TokenKind::Comma,
                TokenKind::Null,
                TokenKind::RBracket,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn all_structural_tokens() {
        assert_eq!(
            kinds("{}[]:,"),
            vec![
                TokenKind::LBrace,
                TokenKind::RBrace,
                TokenKind::LBracket,
                TokenKind::RBracket,
                TokenKind::Colon,
                TokenKind::Comma,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn incomplete_keyword_is_error() {
        // "tru" não é um prefixo aceito de "true".
        assert!(tokenize("tru").is_err());
        assert!(tokenize("nul").is_err());
        assert!(tokenize("fals").is_err());
    }

    #[test]
    fn keyword_mismatch_reports_unexpected_char() {
        let e = err("trux");
        assert!(matches!(e.kind, ParseErrorKind::UnexpectedChar('x')));
    }

    #[test]
    fn describe_matches_expected_labels() {
        assert_eq!(TokenKind::LBrace.describe(), "{");
        assert_eq!(TokenKind::RBrace.describe(), "}");
        assert_eq!(TokenKind::LBracket.describe(), "[");
        assert_eq!(TokenKind::RBracket.describe(), "]");
        assert_eq!(TokenKind::Colon.describe(), ":");
        assert_eq!(TokenKind::Comma.describe(), ",");
        assert_eq!(TokenKind::String(String::new()).describe(), "string");
        assert_eq!(TokenKind::Number(0.0).describe(), "number");
        assert_eq!(TokenKind::True.describe(), "true");
        assert_eq!(TokenKind::False.describe(), "false");
        assert_eq!(TokenKind::Null.describe(), "null");
        assert_eq!(TokenKind::Eof.describe(), "fim da entrada");
    }

    // --- linha/coluna ----------------------------------------------------

    #[test]
    fn positions_on_single_line() {
        let tokens = tokenize("{ }").unwrap();
        assert_eq!((tokens[0].line, tokens[0].col), (1, 1)); // {
        assert_eq!((tokens[1].line, tokens[1].col), (1, 3)); // }
        assert_eq!((tokens[2].line, tokens[2].col), (1, 4)); // Eof
    }

    #[test]
    fn positions_after_newline() {
        let tokens = tokenize("{\n  \"a\"\n}").unwrap();
        // linha 1: '{' na coluna 1
        assert_eq!((tokens[0].line, tokens[0].col), (1, 1));
        // linha 2: "  \"a\"" -> a aspa de abertura está na coluna 3
        assert_eq!((tokens[1].line, tokens[1].col), (2, 3));
        // linha 3: '}' na coluna 1
        assert_eq!((tokens[2].line, tokens[2].col), (3, 1));
    }

    #[test]
    fn multibyte_char_counts_as_one_column() {
        // "é" é multi-byte em UTF-8, mas deve contar como 1 coluna.
        let e = err("\"é\"x");
        // 'x' depois da string fechada: string ocupa colunas 1-3 ("é" tem
        // aspa, é, aspa = 3 caracteres), então 'x' cai na coluna 4.
        assert_eq!((e.line, e.col), (1, 4));
    }

    // --- strings e escapes -------------------------------------------

    #[test]
    fn simple_string() {
        assert_eq!(
            kinds(r#""hello""#),
            vec![TokenKind::String("hello".to_string()), TokenKind::Eof]
        );
    }

    #[test]
    fn each_simple_escape() {
        let cases: &[(&str, &str)] = &[
            (r#""\"""#, "\""),
            (r#""\\""#, "\\"),
            (r#""\/""#, "/"),
            (r#""\b""#, "\u{0008}"),
            (r#""\f""#, "\u{000C}"),
            (r#""\n""#, "\n"),
            (r#""\r""#, "\r"),
            (r#""\t""#, "\t"),
        ];
        for (input, expected) in cases {
            let toks = tokenize(input).unwrap();
            assert_eq!(toks[0].kind, TokenKind::String(expected.to_string()));
        }
    }

    #[test]
    fn surrogate_pair_produces_emoji() {
        // U+1F600 (😀) via par substituto UTF-16: alta 0xD83D + baixa 0xDE00.
        let escaped = "\"\\uD83D\\uDE00\"";
        let toks = tokenize(escaped).unwrap();
        assert_eq!(toks[0].kind, TokenKind::String("😀".to_string()));
    }

    #[test]
    fn bmp_unicode_escape_lowercase_hex() {
        // 'é' (U+00E9) via \u com hex minúsculo.
        let escaped = "\"\\u00e9\"";
        let toks = tokenize(escaped).unwrap();
        assert_eq!(toks[0].kind, TokenKind::String("é".to_string()));
    }

    #[test]
    fn bmp_unicode_escape_uppercase_hex() {
        // 'é' (U+00E9) via \u com hex maiúsculo.
        let escaped = "\"\\u00E9\"";
        let toks = tokenize(escaped).unwrap();
        assert_eq!(toks[0].kind, TokenKind::String("é".to_string()));
    }

    #[test]
    fn lone_high_surrogate_is_invalid() {
        let e = err(r#""\uD83D""#);
        assert!(matches!(e.kind, ParseErrorKind::InvalidUnicodeEscape));
    }

    #[test]
    fn lone_low_surrogate_is_invalid() {
        let e = err(r#""\uDE00""#);
        assert!(matches!(e.kind, ParseErrorKind::InvalidUnicodeEscape));
    }

    #[test]
    fn high_surrogate_followed_by_another_high_is_invalid() {
        let e = err(r#""\uD83D\uD83D""#);
        assert!(matches!(e.kind, ParseErrorKind::InvalidUnicodeEscape));
    }

    #[test]
    fn unicode_escape_with_fewer_than_four_hex_digits() {
        let e = err(r#""\u12""#);
        assert!(matches!(e.kind, ParseErrorKind::InvalidUnicodeEscape));
    }

    #[test]
    fn unicode_escape_with_non_hex_digit() {
        let e = err(r#""\u12zz""#);
        assert!(matches!(e.kind, ParseErrorKind::InvalidUnicodeEscape));
    }

    #[test]
    fn unterminated_string_missing_quote() {
        let e = err(r#""abc"#);
        assert!(matches!(e.kind, ParseErrorKind::UnterminatedString));
    }

    #[test]
    fn unterminated_string_eof_mid_escape() {
        let e = err("\"abc\\");
        assert!(matches!(e.kind, ParseErrorKind::UnterminatedString));
    }

    #[test]
    fn invalid_escape_char() {
        let e = err(r#""\x""#);
        assert!(matches!(e.kind, ParseErrorKind::InvalidEscape('x')));
    }

    #[test]
    fn literal_control_char_in_string_is_error() {
        let input = "\"a\u{0001}b\"";
        let e = err(input);
        assert!(matches!(e.kind, ParseErrorKind::UnexpectedChar('\u{0001}')));
    }

    // --- números -------------------------------------------------------

    #[test]
    fn valid_numbers() {
        let cases: &[(&str, f64)] = &[
            ("0", 0.0),
            ("-0", -0.0),
            ("42", 42.0),
            ("-1.5", -1.5),
            ("1e10", 1e10),
            ("1E+2", 1E+2),
            ("2.5e-3", 2.5e-3),
        ];
        for (input, expected) in cases {
            let toks = tokenize(input).unwrap();
            match &toks[0].kind {
                TokenKind::Number(n) => assert_eq!(n, expected, "input: {input}"),
                other => panic!("esperava Number para {input:?}, veio {other:?}"),
            }
        }
    }

    #[test]
    fn number_out_of_f64_range_becomes_infinity() {
        let toks = tokenize("1e400").unwrap();
        assert_eq!(toks[0].kind, TokenKind::Number(f64::INFINITY));
    }

    #[test]
    fn invalid_numbers() {
        for input in ["01", "1.", ".5", "+1", "1e", "1e+", "-"] {
            let result = tokenize(input);
            assert!(
                matches!(
                    result,
                    Err(ParseError {
                        kind: ParseErrorKind::InvalidNumber,
                        ..
                    })
                ),
                "esperava InvalidNumber para {input:?}, veio {result:?}"
            );
        }
    }

    // --- lixo -----------------------------------------------------------

    #[test]
    fn unexpected_char_for_garbage() {
        let e = err("@");
        assert!(matches!(e.kind, ParseErrorKind::UnexpectedChar('@')));
        assert_eq!((e.line, e.col), (1, 1));
    }
}
