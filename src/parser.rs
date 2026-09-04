//! Parser recursivo descendente: consome a sequência de tokens já lexada
//! e monta a árvore `Value`, um método por regra da gramática.
//!
//! Gramática (EBNF, ver `docs/project.md`):
//!
//! ```text
//! value   = object | array | string | number | "true" | "false" | "null"
//! object  = "{" [ member ("," member)* ] "}"
//! member  = string ":" value
//! array   = "[" [ value ("," value)* ] "]"
//! ```

use crate::error::{ParseError, ParseErrorKind};
use crate::lexer::{Token, TokenKind};
use crate::value::Value;

/// Profundidade máxima de aninhamento de objetos/arrays permitida.
///
/// Um parser recursivo descendente empilha um frame de `parse_value` (e do
/// método específico de `parse_object`/`parse_array`) por nível de
/// aninhamento. Uma entrada adversarial como `[[[[...]]]]` com milhares de
/// níveis estouraria a pilha de chamadas real da máquina e derrubaria o
/// processo inteiro — isso é um crash, não um `Err` tratável, e é
/// justamente o tipo de entrada que suítes de conformidade (JSONTestSuite)
/// testam de propósito. `MAX_DEPTH` transforma esse crash num
/// `ParseErrorKind::DepthLimitExceeded` recuperável. O valor escolhido é
/// conservador — bem menor que o necessário para estourar a pilha real —,
/// não tem nenhum significado semântico além disso.
const MAX_DEPTH: usize = 128;

/// Consome a sequência de tokens já lexada e produz o `Value` correspondente.
///
/// Espera que `tokens` termine com um token `Eof` (responsabilidade do
/// lexer — todo `Vec<Token>` produzido por ele carrega um `Eof` final
/// posicionado logo após o último caractere da entrada). Retorna erro se
/// sobrar qualquer token não consumido depois do valor de topo (`true
/// false`, `{} {}`, `[1] x`, ...) ou se a entrada não contiver nenhum valor
/// (entrada vazia, que produz só `Eof`).
pub(crate) fn parse_tokens(tokens: &[Token]) -> Result<Value, ParseError> {
    // Defensivo: o contrato do lexer garante ao menos um token (`Eof`), mas
    // não custa não confiar em índice fora dos limites caso isso mude.
    if tokens.is_empty() {
        return Err(ParseError::new(ParseErrorKind::UnexpectedEof, 1, 1));
    }

    let mut parser = Parser {
        tokens,
        pos: 0,
        depth: 0,
    };
    let value = parser.parse_value()?;
    parser.expect_eof()?;
    Ok(value)
}

/// Estado do parser: a fatia de tokens, a posição de leitura atual e a
/// profundidade de aninhamento corrente (ver `MAX_DEPTH`).
struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
    depth: usize,
}

impl<'a> Parser<'a> {
    /// Olha o token atual sem consumi-lo.
    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    /// Consome o token atual, avançando o cursor.
    ///
    /// Nunca avança além do último token (`Eof`): olhar `Eof` repetidamente
    /// é seguro, ele nunca "acaba".
    fn advance(&mut self) {
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
    }

    /// Entra num nível de aninhamento (chamado ao iniciar `parse_object`
    /// ou `parse_array`), rejeitando com `DepthLimitExceeded` antes de
    /// estourar a pilha real de chamadas. Ver `MAX_DEPTH`.
    fn enter_scope(&mut self) -> Result<(), ParseError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            let token = self.peek();
            return Err(ParseError::new(
                ParseErrorKind::DepthLimitExceeded,
                token.line,
                token.col,
            ));
        }
        Ok(())
    }

    /// Sai de um nível de aninhamento. Chamado só no caminho de sucesso —
    /// no caminho de erro o parser inteiro aborta, então não há frame para
    /// "desfazer".
    fn exit_scope(&mut self) {
        self.depth -= 1;
    }

    /// Constrói o erro para "token inesperado neste ponto".
    ///
    /// Copia `line`/`col` direto do token atual — o parser não mantém
    /// cursor de posição próprio, essa é a única fonte de posição. Quando
    /// o token atual é `Eof`, o erro correto é `UnexpectedEof` (a entrada
    /// terminou onde um token era esperado), não `UnexpectedToken`; para
    /// qualquer outro token, é `UnexpectedToken` com a descrição do token
    /// encontrado.
    fn error_unexpected(&self, expected: &'static str) -> ParseError {
        let token = self.peek();
        match &token.kind {
            TokenKind::Eof => ParseError::new(ParseErrorKind::UnexpectedEof, token.line, token.col),
            kind => ParseError::new(
                ParseErrorKind::UnexpectedToken {
                    expected,
                    found: kind.describe(),
                },
                token.line,
                token.col,
            ),
        }
    }

    /// `value = object | array | string | number | "true" | "false" | "null"`.
    fn parse_value(&mut self) -> Result<Value, ParseError> {
        match &self.peek().kind {
            TokenKind::LBrace => self.parse_object(),
            TokenKind::LBracket => self.parse_array(),
            TokenKind::String(s) => {
                let s = s.clone();
                self.advance();
                Ok(Value::String(s))
            }
            TokenKind::Number(n) => {
                let n = *n;
                self.advance();
                Ok(Value::Number(n))
            }
            TokenKind::True => {
                self.advance();
                Ok(Value::Bool(true))
            }
            TokenKind::False => {
                self.advance();
                Ok(Value::Bool(false))
            }
            TokenKind::Null => {
                self.advance();
                Ok(Value::Null)
            }
            // Cobre também `Eof`: `error_unexpected` sabe transformar isso
            // em `UnexpectedEof` em vez de `UnexpectedToken`.
            _ => Err(self.error_unexpected("valor")),
        }
    }

    /// `object = "{" [ member ("," member)* ] "}"`, `member = string ":" value`.
    ///
    /// Objeto vazio (`{}`) é válido. Trailing comma (`{"a":1,}`) não é
    /// tratada como caso especial: depois da vírgula o laço volta a exigir
    /// uma chave string, e `}` nesse ponto já produz o erro certo
    /// naturalmente.
    ///
    /// Chaves duplicadas (`{"a":1,"a":2}`) **não** passam por dedup — as
    /// duas entradas são mantidas em `entries`, na ordem em que apareceram.
    /// RFC 8259 não define esse caso; é decisão consciente do projeto,
    /// coerente com `Value::Object` ser `Vec` em vez de `HashMap`.
    fn parse_object(&mut self) -> Result<Value, ParseError> {
        self.enter_scope()?;
        self.advance(); // consome '{'

        let mut entries = Vec::new();

        if !matches!(self.peek().kind, TokenKind::RBrace) {
            loop {
                let key = self.expect_string()?;
                self.expect_colon()?;
                let value = self.parse_value()?;
                entries.push((key, value));

                match &self.peek().kind {
                    TokenKind::Comma => self.advance(),
                    TokenKind::RBrace => break,
                    _ => return Err(self.error_unexpected("\",\" ou \"}\"")),
                }
            }
        }

        self.advance(); // consome '}' (garantido pelo laço acima ou pelo `if` vazio)
        self.exit_scope();
        Ok(Value::Object(entries))
    }

    /// `array = "[" [ value ("," value)* ] "]"`.
    ///
    /// Array vazio (`[]`) é válido. Trailing comma (`[1,]`) também não é
    /// caso especial: depois da vírgula o laço volta a exigir um valor, e
    /// `]` nesse ponto não é início de valor válido, então o erro certo
    /// ("valor") sai naturalmente de `parse_value`.
    fn parse_array(&mut self) -> Result<Value, ParseError> {
        self.enter_scope()?;
        self.advance(); // consome '['

        let mut items = Vec::new();

        if !matches!(self.peek().kind, TokenKind::RBracket) {
            loop {
                let value = self.parse_value()?;
                items.push(value);

                match &self.peek().kind {
                    TokenKind::Comma => self.advance(),
                    TokenKind::RBracket => break,
                    _ => return Err(self.error_unexpected("\",\" ou \"]\"")),
                }
            }
        }

        self.advance(); // consome ']' (garantido pelo laço acima ou pelo `if` vazio)
        self.exit_scope();
        Ok(Value::Array(items))
    }

    /// Espera um token `string` no ponto atual (chave de objeto) e o
    /// consome, devolvendo a `String` interna. `{a: 1}` (chave sem aspas)
    /// cai aqui e produz erro — JSON estrito exige chave entre aspas.
    fn expect_string(&mut self) -> Result<String, ParseError> {
        match &self.peek().kind {
            TokenKind::String(s) => {
                let s = s.clone();
                self.advance();
                Ok(s)
            }
            _ => Err(self.error_unexpected("chave de objeto entre aspas")),
        }
    }

    /// Espera e consome um `:` entre chave e valor de um membro de objeto.
    fn expect_colon(&mut self) -> Result<(), ParseError> {
        match &self.peek().kind {
            TokenKind::Colon => {
                self.advance();
                Ok(())
            }
            _ => Err(self.error_unexpected(":")),
        }
    }

    /// Verifica que não sobrou nenhum token além de `Eof` depois do valor
    /// de topo. Sem essa checagem, `true false` ou `{} {}` "terminariam
    /// com sucesso" ao reconhecer só o primeiro valor válido.
    fn expect_eof(&self) -> Result<(), ParseError> {
        match &self.peek().kind {
            TokenKind::Eof => Ok(()),
            _ => Err(self.error_unexpected("fim da entrada")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tok(kind: TokenKind, line: usize, col: usize) -> Token {
        Token { kind, line, col }
    }

    fn eof(line: usize, col: usize) -> Token {
        tok(TokenKind::Eof, line, col)
    }

    fn parse(tokens: &[Token]) -> Result<Value, ParseError> {
        parse_tokens(tokens)
    }

    // --- literais isolados ---------------------------------------------

    #[test]
    fn literal_true() {
        let tokens = vec![tok(TokenKind::True, 1, 1), eof(1, 5)];
        assert_eq!(parse(&tokens), Ok(Value::Bool(true)));
    }

    #[test]
    fn literal_false() {
        let tokens = vec![tok(TokenKind::False, 1, 1), eof(1, 6)];
        assert_eq!(parse(&tokens), Ok(Value::Bool(false)));
    }

    #[test]
    fn literal_null() {
        let tokens = vec![tok(TokenKind::Null, 1, 1), eof(1, 5)];
        assert_eq!(parse(&tokens), Ok(Value::Null));
    }

    #[test]
    fn literal_string() {
        let tokens = vec![tok(TokenKind::String("oi".to_string()), 1, 1), eof(1, 5)];
        assert_eq!(parse(&tokens), Ok(Value::String("oi".to_string())));
    }

    #[test]
    fn literal_number() {
        let tokens = vec![tok(TokenKind::Number(42.5), 1, 1), eof(1, 5)];
        assert_eq!(parse(&tokens), Ok(Value::Number(42.5)));
    }

    // --- objeto e array vazios ------------------------------------------

    #[test]
    fn objeto_vazio() {
        let tokens = vec![
            tok(TokenKind::LBrace, 1, 1),
            tok(TokenKind::RBrace, 1, 2),
            eof(1, 3),
        ];
        assert_eq!(parse(&tokens), Ok(Value::Object(vec![])));
    }

    #[test]
    fn array_vazio() {
        let tokens = vec![
            tok(TokenKind::LBracket, 1, 1),
            tok(TokenKind::RBracket, 1, 2),
            eof(1, 3),
        ];
        assert_eq!(parse(&tokens), Ok(Value::Array(vec![])));
    }

    // --- aninhamento ------------------------------------------------------

    #[test]
    fn objeto_aninhado_em_array_aninhado_em_objeto() {
        // {"a": [1, {"b": null}]}
        let tokens = vec![
            tok(TokenKind::LBrace, 1, 1),
            tok(TokenKind::String("a".to_string()), 1, 2),
            tok(TokenKind::Colon, 1, 5),
            tok(TokenKind::LBracket, 1, 7),
            tok(TokenKind::Number(1.0), 1, 8),
            tok(TokenKind::Comma, 1, 9),
            tok(TokenKind::LBrace, 1, 11),
            tok(TokenKind::String("b".to_string()), 1, 12),
            tok(TokenKind::Colon, 1, 15),
            tok(TokenKind::Null, 1, 17),
            tok(TokenKind::RBrace, 1, 21),
            tok(TokenKind::RBracket, 1, 22),
            tok(TokenKind::RBrace, 1, 23),
            eof(1, 24),
        ];

        let esperado = Value::Object(vec![(
            "a".to_string(),
            Value::Array(vec![
                Value::Number(1.0),
                Value::Object(vec![("b".to_string(), Value::Null)]),
            ]),
        )]);

        assert_eq!(parse(&tokens), Ok(esperado));
    }

    // --- ordem e duplicidade de chaves ----------------------------------

    #[test]
    fn ordem_das_chaves_e_preservada() {
        // {"z": 1, "a": 2, "m": 3}
        let tokens = vec![
            tok(TokenKind::LBrace, 1, 1),
            tok(TokenKind::String("z".to_string()), 1, 2),
            tok(TokenKind::Colon, 1, 5),
            tok(TokenKind::Number(1.0), 1, 6),
            tok(TokenKind::Comma, 1, 7),
            tok(TokenKind::String("a".to_string()), 1, 9),
            tok(TokenKind::Colon, 1, 12),
            tok(TokenKind::Number(2.0), 1, 13),
            tok(TokenKind::Comma, 1, 14),
            tok(TokenKind::String("m".to_string()), 1, 16),
            tok(TokenKind::Colon, 1, 19),
            tok(TokenKind::Number(3.0), 1, 20),
            tok(TokenKind::RBrace, 1, 21),
            eof(1, 22),
        ];

        let esperado = Value::Object(vec![
            ("z".to_string(), Value::Number(1.0)),
            ("a".to_string(), Value::Number(2.0)),
            ("m".to_string(), Value::Number(3.0)),
        ]);

        assert_eq!(parse(&tokens), Ok(esperado));
    }

    #[test]
    fn chaves_duplicadas_mantem_as_duas_entradas() {
        // {"a":1,"a":2}
        let tokens = vec![
            tok(TokenKind::LBrace, 1, 1),
            tok(TokenKind::String("a".to_string()), 1, 2),
            tok(TokenKind::Colon, 1, 5),
            tok(TokenKind::Number(1.0), 1, 6),
            tok(TokenKind::Comma, 1, 7),
            tok(TokenKind::String("a".to_string()), 1, 8),
            tok(TokenKind::Colon, 1, 11),
            tok(TokenKind::Number(2.0), 1, 12),
            tok(TokenKind::RBrace, 1, 13),
            eof(1, 14),
        ];

        let esperado = Value::Object(vec![
            ("a".to_string(), Value::Number(1.0)),
            ("a".to_string(), Value::Number(2.0)),
        ]);

        assert_eq!(parse(&tokens), Ok(esperado));
    }

    // --- trailing comma (inválida: JSON estrito) ------------------------

    #[test]
    fn trailing_comma_em_objeto_e_erro() {
        // {"a":1,}
        let tokens = vec![
            tok(TokenKind::LBrace, 1, 1),
            tok(TokenKind::String("a".to_string()), 1, 2),
            tok(TokenKind::Colon, 1, 5),
            tok(TokenKind::Number(1.0), 1, 6),
            tok(TokenKind::Comma, 1, 7),
            tok(TokenKind::RBrace, 1, 8),
            eof(1, 9),
        ];

        let err = parse(&tokens).unwrap_err();
        assert_eq!(
            err.kind,
            ParseErrorKind::UnexpectedToken {
                expected: "chave de objeto entre aspas",
                found: TokenKind::RBrace.describe(),
            }
        );
        assert_eq!((err.line, err.col), (1, 8));
    }

    #[test]
    fn trailing_comma_em_array_e_erro() {
        // [1,]
        let tokens = vec![
            tok(TokenKind::LBracket, 1, 1),
            tok(TokenKind::Number(1.0), 1, 2),
            tok(TokenKind::Comma, 1, 3),
            tok(TokenKind::RBracket, 1, 4),
            eof(1, 5),
        ];

        let err = parse(&tokens).unwrap_err();
        assert_eq!(
            err.kind,
            ParseErrorKind::UnexpectedToken {
                expected: "valor",
                found: TokenKind::RBracket.describe(),
            }
        );
        assert_eq!((err.line, err.col), (1, 4));
    }

    // --- erros de gramática pontuais -------------------------------------

    #[test]
    fn dois_pontos_faltando_e_erro() {
        // {"a" 1}
        let tokens = vec![
            tok(TokenKind::LBrace, 1, 1),
            tok(TokenKind::String("a".to_string()), 1, 2),
            tok(TokenKind::Number(1.0), 1, 6),
            tok(TokenKind::RBrace, 1, 7),
            eof(1, 8),
        ];

        let err = parse(&tokens).unwrap_err();
        assert_eq!(
            err.kind,
            ParseErrorKind::UnexpectedToken {
                expected: ":",
                found: TokenKind::Number(1.0).describe(),
            }
        );
        assert_eq!((err.line, err.col), (1, 6));
    }

    #[test]
    fn chave_nao_string_e_erro() {
        // {1:2}
        let tokens = vec![
            tok(TokenKind::LBrace, 1, 1),
            tok(TokenKind::Number(1.0), 1, 2),
            tok(TokenKind::Colon, 1, 3),
            tok(TokenKind::Number(2.0), 1, 4),
            tok(TokenKind::RBrace, 1, 5),
            eof(1, 6),
        ];

        let err = parse(&tokens).unwrap_err();
        assert_eq!(
            err.kind,
            ParseErrorKind::UnexpectedToken {
                expected: "chave de objeto entre aspas",
                found: TokenKind::Number(1.0).describe(),
            }
        );
        assert_eq!((err.line, err.col), (1, 2));
    }

    #[test]
    fn virgula_sobrando_em_array_e_erro() {
        // [1,,2]
        let tokens = vec![
            tok(TokenKind::LBracket, 1, 1),
            tok(TokenKind::Number(1.0), 1, 2),
            tok(TokenKind::Comma, 1, 3),
            tok(TokenKind::Comma, 1, 4),
            tok(TokenKind::Number(2.0), 1, 5),
            tok(TokenKind::RBracket, 1, 6),
            eof(1, 7),
        ];

        let err = parse(&tokens).unwrap_err();
        assert_eq!(
            err.kind,
            ParseErrorKind::UnexpectedToken {
                expected: "valor",
                found: TokenKind::Comma.describe(),
            }
        );
        assert_eq!((err.line, err.col), (1, 4));
    }

    #[test]
    fn colchete_nao_fechado_e_unexpected_eof() {
        // [1
        let tokens = vec![
            tok(TokenKind::LBracket, 1, 1),
            tok(TokenKind::Number(1.0), 1, 2),
            eof(1, 3),
        ];

        let err = parse(&tokens).unwrap_err();
        assert_eq!(err.kind, ParseErrorKind::UnexpectedEof);
        assert_eq!((err.line, err.col), (1, 3));
    }

    // --- token sobrando depois do valor de topo -------------------------

    #[test]
    fn token_sobrando_depois_do_valor_de_topo_e_erro() {
        // true false
        let tokens = vec![
            tok(TokenKind::True, 1, 1),
            tok(TokenKind::False, 1, 6),
            eof(1, 11),
        ];

        let err = parse(&tokens).unwrap_err();
        assert_eq!(
            err.kind,
            ParseErrorKind::UnexpectedToken {
                expected: "fim da entrada",
                found: TokenKind::False.describe(),
            }
        );
        assert_eq!((err.line, err.col), (1, 6));
    }

    #[test]
    fn dois_objetos_seguidos_e_erro() {
        // {} {}
        let tokens = vec![
            tok(TokenKind::LBrace, 1, 1),
            tok(TokenKind::RBrace, 1, 2),
            tok(TokenKind::LBrace, 1, 4),
            tok(TokenKind::RBrace, 1, 5),
            eof(1, 6),
        ];

        let err = parse(&tokens).unwrap_err();
        assert_eq!(
            err.kind,
            ParseErrorKind::UnexpectedToken {
                expected: "fim da entrada",
                found: TokenKind::LBrace.describe(),
            }
        );
        assert_eq!((err.line, err.col), (1, 4));
    }

    // --- entrada vazia ----------------------------------------------------

    #[test]
    fn entrada_so_com_eof_e_unexpected_eof() {
        let tokens = vec![eof(1, 1)];
        let err = parse(&tokens).unwrap_err();
        assert_eq!(err.kind, ParseErrorKind::UnexpectedEof);
        assert_eq!((err.line, err.col), (1, 1));
    }

    #[test]
    fn fatia_de_tokens_vazia_e_unexpected_eof() {
        // Defensivo: contrato do lexer garante >= 1 token, mas o parser
        // não deve entrar em pânico se isso um dia não valer.
        let tokens: Vec<Token> = vec![];
        let err = parse(&tokens).unwrap_err();
        assert_eq!(err.kind, ParseErrorKind::UnexpectedEof);
        assert_eq!((err.line, err.col), (1, 1));
    }

    // --- limite de profundidade -----------------------------------------

    #[test]
    fn aninhamento_alem_do_limite_retorna_depth_limit_exceeded() {
        let depth = MAX_DEPTH + 10;
        let mut tokens = Vec::with_capacity(depth * 2 + 1);

        for i in 0..depth {
            tokens.push(tok(TokenKind::LBracket, 1, i + 1));
        }
        for i in 0..depth {
            tokens.push(tok(TokenKind::RBracket, 1, depth + i + 1));
        }
        tokens.push(eof(1, depth * 2 + 1));

        let err = parse(&tokens).unwrap_err();
        assert_eq!(err.kind, ParseErrorKind::DepthLimitExceeded);
    }

    #[test]
    fn aninhamento_no_limite_exato_e_aceito() {
        // Exatamente MAX_DEPTH níveis deve continuar válido — só
        // MAX_DEPTH + 1 deve falhar.
        let depth = MAX_DEPTH;
        let mut tokens = Vec::with_capacity(depth * 2 + 1);

        for i in 0..depth {
            tokens.push(tok(TokenKind::LBracket, 1, i + 1));
        }
        for i in 0..depth {
            tokens.push(tok(TokenKind::RBracket, 1, depth + i + 1));
        }
        tokens.push(eof(1, depth * 2 + 1));

        assert!(parse(&tokens).is_ok());
    }

    // --- posição exata vinda do token -----------------------------------

    #[test]
    fn erro_carrega_linha_e_coluna_exatas_do_token() {
        // {\n  "a" 1\n}  -- ":" faltando na linha 2
        let tokens = vec![
            tok(TokenKind::LBrace, 1, 1),
            tok(TokenKind::String("a".to_string()), 2, 3),
            tok(TokenKind::Number(1.0), 2, 8),
            tok(TokenKind::RBrace, 3, 1),
            eof(3, 2),
        ];

        let err = parse(&tokens).unwrap_err();
        assert_eq!(err.line, 2);
        assert_eq!(err.col, 8);
        assert_eq!(
            err.kind,
            ParseErrorKind::UnexpectedToken {
                expected: ":",
                found: TokenKind::Number(1.0).describe(),
            }
        );
    }
}
