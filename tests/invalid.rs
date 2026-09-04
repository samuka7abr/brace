//! Testes de integração: entradas inválidas, via API pública (`brace::parse`).
//!
//! O foco aqui não é só "deu erro", mas a posição exata (`line`/`col`) onde o
//! erro foi detectado, sempre que a contagem é inequívoca. Onde a posição
//! depende de uma escolha de implementação que não está fixada pela spec
//! (por exemplo, o ponto exato de falha ao lexar um token que só é
//! reconhecido como inválido perto do fim, ou o `ParseErrorKind` usado para
//! EOF inesperado no meio de uma estrutura), só o que é seguro é asserido —
//! isso está anotado em cada caso.

use brace::ParseErrorKind;

fn erro(input: &str) -> brace::ParseError {
    brace::parse(input).expect_err("esperava erro de parse")
}

// ===================================================================
// Entrada vazia / só espaço em branco
// ===================================================================

#[test]
fn entrada_vazia_e_eof_na_posicao_inicial() {
    // Nenhum caractere é consumido: o cursor nunca saiu de (1, 1).
    let err = erro("");
    assert_eq!(err.kind, ParseErrorKind::UnexpectedEof);
    assert_eq!((err.line, err.col), (1, 1));
}

#[test]
fn entrada_so_espacos_e_eof_apos_o_ultimo_espaco() {
    // "   " tem 3 espaços: col avança 1 -> 2 -> 3 -> 4, Eof fica em col 4.
    let err = erro("   ");
    assert_eq!(err.kind, ParseErrorKind::UnexpectedEof);
    assert_eq!((err.line, err.col), (1, 4));
}

// ===================================================================
// Objeto
// ===================================================================

#[test]
fn objeto_sem_dois_pontos_apos_a_chave() {
    // {"a" 1}
    // col: 1{ 2" 3a 4" 5(espaço) 6'1' 7}  -> token inesperado "1" na col 6
    let err = erro("{\"a\" 1}");
    assert!(matches!(err.kind, ParseErrorKind::UnexpectedToken { .. }));
    assert_eq!((err.line, err.col), (1, 6));
}

#[test]
fn objeto_com_virgula_sobrando_antes_do_fecha_chaves() {
    // {"a":1,}
    // col: 1{ 2" 3a 4" 5: 6'1' 7, 8}  -> depois da vírgula sobra só "}", na col 8
    let err = erro("{\"a\":1,}");
    assert!(matches!(err.kind, ParseErrorKind::UnexpectedToken { .. }));
    assert_eq!((err.line, err.col), (1, 8));
}

#[test]
fn objeto_com_chave_sem_aspas() {
    // {a: 1}
    // col: 1{ 2a  -- 'a' não inicia nenhum token válido (não é t/f/n, aspas, etc.)
    let err = erro("{a: 1}");
    assert_eq!(err.kind, ParseErrorKind::UnexpectedChar('a'));
    assert_eq!((err.line, err.col), (1, 2));
}

#[test]
fn objeto_com_chave_string_nao_fechada() {
    // {"a: 1}  -- a aspa de abertura na col 2 nunca fecha; todo o resto
    // (incluindo o "}") é engolido como conteúdo da string até o fim da
    // entrada. A posição exata reportada por UnterminatedString (início da
    // string vs. fim da entrada) não está fixada pela spec, então só o kind
    // é asserido.
    let err = erro("{\"a: 1}");
    assert_eq!(err.kind, ParseErrorKind::UnterminatedString);
}

#[test]
fn objeto_sem_fecha_chaves() {
    // {"a":1  -- termina no meio do objeto.
    // col: 1{ 2" 3a 4" 5: 6'1'  -> Eof logo após o último caractere consumido: col 7.
    // O kind pode ser UnexpectedEof ou UnexpectedToken{found: "fim da entrada"}
    // dependendo de como o parser trata o token Eof — não fixado aqui.
    let err = erro("{\"a\":1");
    assert!(matches!(
        err.kind,
        ParseErrorKind::UnexpectedEof | ParseErrorKind::UnexpectedToken { .. }
    ));
    assert_eq!((err.line, err.col), (1, 7));
}

// ===================================================================
// Array
// ===================================================================

#[test]
fn array_sem_fecha_colchete() {
    // [1,2  -- col: 1[ 2'1' 3, 4'2'  -> Eof em col 5
    let err = erro("[1,2");
    assert!(matches!(
        err.kind,
        ParseErrorKind::UnexpectedEof | ParseErrorKind::UnexpectedToken { .. }
    ));
    assert_eq!((err.line, err.col), (1, 5));
}

#[test]
fn array_com_virgula_sobrando_antes_do_fecha_colchete() {
    // [1,2,]  -- col: 1[ 2'1' 3, 4'2' 5, 6]  -> "]" inesperado na col 6
    let err = erro("[1,2,]");
    assert!(matches!(err.kind, ParseErrorKind::UnexpectedToken { .. }));
    assert_eq!((err.line, err.col), (1, 6));
}

#[test]
fn array_com_virgula_dupla() {
    // [1,,2]  -- col: 1[ 2'1' 3, 4','  -> segunda vírgula onde um valor era esperado, col 4
    let err = erro("[1,,2]");
    assert!(matches!(err.kind, ParseErrorKind::UnexpectedToken { .. }));
    assert_eq!((err.line, err.col), (1, 4));
}

// ===================================================================
// String
// ===================================================================

#[test]
fn string_com_aspas_nao_fechada() {
    // "abc  -- nunca fecha; posição exata ambígua (ver caso do objeto acima), só o kind é asserido.
    let err = erro("\"abc");
    assert_eq!(err.kind, ParseErrorKind::UnterminatedString);
}

#[test]
fn string_com_escape_desconhecido() {
    // "\x" -- \x não é um escape válido em JSON.
    let err = erro("\"\\x\"");
    assert_eq!(err.kind, ParseErrorKind::InvalidEscape('x'));
}

#[test]
fn string_com_caractere_de_controle_literal() {
    // TAB (U+0009) literal, não escapado, dentro da string — proibido pela
    // RFC 8259 (caracteres de controle U+0000-U+001F precisam ser
    // escapados). ParseErrorKind não tem uma variante dedicada a esse caso,
    // então o kind exato não é fixado aqui — só que a entrada é rejeitada.
    let entrada = "\"a\u{9}b\"";
    erro(entrada);
}

// ===================================================================
// \uXXXX malformado
// ===================================================================

#[test]
fn unicode_escape_com_menos_de_4_digitos_hex() {
    // "\u12" -- a aspa de fechamento chega depois de só 2 dígitos hex.
    let err = erro("\"\\u12\"");
    assert_eq!(err.kind, ParseErrorKind::InvalidUnicodeEscape);
}

#[test]
fn unicode_escape_com_digito_nao_hexadecimal() {
    // "\u12g4" -- 'g' não é dígito hexadecimal.
    let err = erro("\"\\u12g4\"");
    assert_eq!(err.kind, ParseErrorKind::InvalidUnicodeEscape);
}

#[test]
fn unicode_escape_substituto_alto_solto() {
    // "\uD83D" -- substituto alto sem o substituto baixo correspondente.
    let err = erro("\"\\uD83D\"");
    assert_eq!(err.kind, ParseErrorKind::InvalidUnicodeEscape);
}

#[test]
fn unicode_escape_substituto_baixo_solto() {
    // "\uDE00" -- substituto baixo sem um substituto alto antes.
    let err = erro("\"\\uDE00\"");
    assert_eq!(err.kind, ParseErrorKind::InvalidUnicodeEscape);
}

// ===================================================================
// Números que str::parse::<f64>() aceitaria, mas o RFC 8259 proíbe
// ===================================================================

#[test]
fn numero_com_zero_a_esquerda_e_invalido() {
    let err = erro("01");
    assert_eq!(err.kind, ParseErrorKind::InvalidNumber);
}

#[test]
fn numero_terminado_em_ponto_sem_fracao_e_invalido() {
    let err = erro("1.");
    assert_eq!(err.kind, ParseErrorKind::InvalidNumber);
}

#[test]
fn numero_iniciado_em_ponto_sem_parte_inteira_e_invalido() {
    let err = erro(".5");
    assert_eq!(err.kind, ParseErrorKind::InvalidNumber);
}

#[test]
fn numero_com_sinal_positivo_explicito_e_invalido() {
    let err = erro("+1");
    assert_eq!(err.kind, ParseErrorKind::InvalidNumber);
}

#[test]
fn numero_com_expoente_sem_digitos_e_invalido() {
    let err = erro("1e");
    assert_eq!(err.kind, ParseErrorKind::InvalidNumber);
}

#[test]
fn numero_com_expoente_so_com_sinal_e_invalido() {
    let err = erro("1e+");
    assert_eq!(err.kind, ParseErrorKind::InvalidNumber);
}

#[test]
fn sinal_de_menos_sozinho_e_invalido() {
    let err = erro("-");
    assert_eq!(err.kind, ParseErrorKind::InvalidNumber);
}

// ===================================================================
// Literal truncado
// ===================================================================

#[test]
fn literal_true_truncado() {
    // "tru" -- o kind exato (UnexpectedChar no ponto da divergência, ou
    // UnexpectedEof se o mismatch só é percebido ao faltar o 'e' final) não
    // está fixado pela spec; só se garante que é rejeitado.
    erro("tru");
}

#[test]
fn literal_null_truncado() {
    erro("nul");
}

#[test]
fn literal_false_truncado() {
    erro("fals");
}

// ===================================================================
// Lixo
// ===================================================================

#[test]
fn caractere_arroba_e_invalido() {
    let err = erro("@");
    assert_eq!(err.kind, ParseErrorKind::UnexpectedChar('@'));
    assert_eq!((err.line, err.col), (1, 1));
}

#[test]
fn aspas_simples_nao_sao_validas_em_json() {
    let err = erro("'aspas simples'");
    assert_eq!(err.kind, ParseErrorKind::UnexpectedChar('\''));
    assert_eq!((err.line, err.col), (1, 1));
}

// ===================================================================
// Conteúdo sobrando depois de um valor completo
// ===================================================================

#[test]
fn conteudo_sobrando_apos_true_false() {
    // "true false" -- "true" ocupa as cols 1-4, o espaço a col 5,
    // "false" começa na col 6.
    let err = erro("true false");
    assert!(matches!(err.kind, ParseErrorKind::UnexpectedToken { .. }));
    assert_eq!((err.line, err.col), (1, 6));
}

#[test]
fn conteudo_sobrando_apos_objeto_vazio() {
    // "{} {}" -- col: 1{ 2} 3(espaço) 4{  -> segundo objeto começa na col 4
    let err = erro("{} {}");
    assert!(matches!(err.kind, ParseErrorKind::UnexpectedToken { .. }));
    assert_eq!((err.line, err.col), (1, 4));
}

#[test]
fn conteudo_sobrando_apos_array_e_erro_de_lexer() {
    // "[1] x" -- col: 1[ 2'1' 3] 4(espaço) 5'x'
    // 'x' não inicia nenhum token válido. Como a tokenização roda sobre a
    // entrada inteira antes do parser rodar (ver src/lib.rs: `lexer::tokenize`
    // é chamado por completo antes de `parser::parse_tokens`), o erro é
    // detectado na fase de lexing como UnexpectedChar, não como "conteúdo
    // sobrando" pelo parser.
    let err = erro("[1] x");
    assert_eq!(err.kind, ParseErrorKind::UnexpectedChar('x'));
    assert_eq!((err.line, err.col), (1, 5));
}

// ===================================================================
// Profundidade de aninhamento além do limite
// ===================================================================

#[test]
fn aninhamento_extremo_produz_erro_tratavel_sem_estourar_pilha() {
    // 500 colchetes de abertura, sem nenhum de fechamento: deve ser
    // rejeitado com DepthLimitExceeded, não travar nem estourar a pilha.
    let entrada = "[".repeat(500);
    let err = erro(&entrada);
    assert_eq!(err.kind, ParseErrorKind::DepthLimitExceeded);
}

// ===================================================================
// Casos multi-linha (posição exata)
// ===================================================================

#[test]
fn erro_multilinha_do_enunciado() {
    // linha 1: "{"
    // linha 2: "  \"a\": ," -> col 1(espaço) 2(espaço) 3" 4a 5" 6: 7(espaço) 8,
    // o valor esperado depois de ":" é um valor; encontra "," na linha 2, col 8.
    let err = erro("{\n  \"a\": ,\n}");
    assert!(matches!(err.kind, ParseErrorKind::UnexpectedToken { .. }));
    assert_eq!((err.line, err.col), (2, 8));
}

#[test]
fn erro_multilinha_com_tab_antes_da_chave() {
    // linha 1: "{"
    // linha 2: "\t\"a\" 1" -> col 1'\t' 2" 3a 4" 5(espaço) 6'1'
    // dois-pontos faltando: token inesperado "1" na linha 2, col 6.
    let err = erro("{\n\t\"a\" 1\n}");
    assert!(matches!(err.kind, ParseErrorKind::UnexpectedToken { .. }));
    assert_eq!((err.line, err.col), (2, 6));
}
