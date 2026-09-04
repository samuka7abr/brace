//! Testes de integração: entradas válidas, via API pública (`brace::parse`).
//!
//! Cada teste fixa um documento JSON de entrada e compara o `Value` retornado
//! com a árvore esperada, construída à mão.

use brace::Value;

// ===================================================================
// Literais isolados
// ===================================================================

#[test]
fn literal_null() {
    assert_eq!(brace::parse("null").unwrap(), Value::Null);
}

#[test]
fn literal_true() {
    assert_eq!(brace::parse("true").unwrap(), Value::Bool(true));
}

#[test]
fn literal_false() {
    assert_eq!(brace::parse("false").unwrap(), Value::Bool(false));
}

#[test]
fn literal_string_simples() {
    assert_eq!(
        brace::parse(r#""ola mundo""#).unwrap(),
        Value::String("ola mundo".to_string())
    );
}

#[test]
fn literal_numero_simples() {
    assert_eq!(brace::parse("123").unwrap(), Value::Number(123.0));
}

// ===================================================================
// Objeto e array vazios
// ===================================================================

#[test]
fn objeto_vazio() {
    assert_eq!(brace::parse("{}").unwrap(), Value::Object(vec![]));
}

#[test]
fn array_vazio() {
    assert_eq!(brace::parse("[]").unwrap(), Value::Array(vec![]));
}

// ===================================================================
// Documento aninhado completo, com todos os tipos de Value presentes
// ===================================================================

#[test]
fn documento_aninhado_com_todos_os_tipos_de_value() {
    let entrada = r#"
    {
        "nome": "brace",
        "versao": 1,
        "ativo": true,
        "meta": null,
        "tags": ["json", "parser", 42, false, null],
        "config": {
            "nested": {
                "deep": [1, 2, 3]
            }
        }
    }
    "#;

    let esperado = Value::Object(vec![
        ("nome".to_string(), Value::String("brace".to_string())),
        ("versao".to_string(), Value::Number(1.0)),
        ("ativo".to_string(), Value::Bool(true)),
        ("meta".to_string(), Value::Null),
        (
            "tags".to_string(),
            Value::Array(vec![
                Value::String("json".to_string()),
                Value::String("parser".to_string()),
                Value::Number(42.0),
                Value::Bool(false),
                Value::Null,
            ]),
        ),
        (
            "config".to_string(),
            Value::Object(vec![(
                "nested".to_string(),
                Value::Object(vec![(
                    "deep".to_string(),
                    Value::Array(vec![
                        Value::Number(1.0),
                        Value::Number(2.0),
                        Value::Number(3.0),
                    ]),
                )]),
            )]),
        ),
    ]);

    assert_eq!(brace::parse(entrada).unwrap(), esperado);
}

// ===================================================================
// Ordem das chaves preservada na ordem do texto
// ===================================================================

#[test]
fn ordem_das_chaves_e_preservada_na_ordem_do_texto() {
    let entrada = r#"{"z": 1, "a": 2, "m": 3}"#;
    let valor = brace::parse(entrada).unwrap();

    match valor {
        Value::Object(pares) => {
            let chaves: Vec<&str> = pares.iter().map(|(k, _)| k.as_str()).collect();
            assert_eq!(chaves, vec!["z", "a", "m"]);
        }
        outro => panic!("esperava Value::Object, obteve {outro:?}"),
    }
}

// ===================================================================
// Chaves duplicadas mantêm as duas entradas, na ordem em que apareceram
// ===================================================================

#[test]
fn chaves_duplicadas_mantem_as_duas_entradas_na_ordem() {
    let entrada = r#"{"a":1,"a":2}"#;
    let esperado = Value::Object(vec![
        ("a".to_string(), Value::Number(1.0)),
        ("a".to_string(), Value::Number(2.0)),
    ]);

    assert_eq!(brace::parse(entrada).unwrap(), esperado);
}

// ===================================================================
// Espaço em branco irrelevante em toda parte
// ===================================================================

#[test]
fn espaco_em_branco_irrelevante_antes_e_depois_do_valor() {
    let entrada = "  \n\t true \t\n  ";
    assert_eq!(brace::parse(entrada).unwrap(), Value::Bool(true));
}

#[test]
fn espaco_em_branco_irrelevante_entre_tokens_de_objeto_e_array() {
    let entrada = "{\n\t\"a\"\t:\n1,\n\t\"b\"  :  [ 1 ,\n2 ]\n}";
    let esperado = Value::Object(vec![
        ("a".to_string(), Value::Number(1.0)),
        (
            "b".to_string(),
            Value::Array(vec![Value::Number(1.0), Value::Number(2.0)]),
        ),
    ]);

    assert_eq!(brace::parse(entrada).unwrap(), esperado);
}

// ===================================================================
// Números em cada forma da gramática
// ===================================================================

#[test]
fn numero_zero() {
    assert_eq!(brace::parse("0").unwrap(), Value::Number(0.0));
}

#[test]
fn numero_zero_negativo() {
    // -0.0 == 0.0 para f64 (IEEE 754), então a comparação passa independente
    // de o parser preservar o sinal negativo internamente ou não.
    assert_eq!(brace::parse("-0").unwrap(), Value::Number(0.0));
}

#[test]
fn numero_inteiro_positivo() {
    assert_eq!(brace::parse("42").unwrap(), Value::Number(42.0));
}

#[test]
fn numero_fracao_negativa() {
    assert_eq!(brace::parse("-1.5").unwrap(), Value::Number(-1.5));
}

#[test]
fn numero_notacao_cientifica_minuscula() {
    assert_eq!(brace::parse("1e10").unwrap(), Value::Number(1e10));
}

#[test]
fn numero_notacao_cientifica_maiuscula_com_sinal_de_mais() {
    assert_eq!(brace::parse("1E+2").unwrap(), Value::Number(100.0));
}

#[test]
fn numero_fracao_com_expoente_negativo() {
    assert_eq!(brace::parse("2.5e-3").unwrap(), Value::Number(0.0025));
}

// ===================================================================
// Escapes de string
// ===================================================================

#[test]
fn string_com_todos_os_escapes_simples() {
    // Texto JSON de entrada (18 caracteres): "  \"  \\  \/  \b  \f  \n  \r  \t  "
    // (sem os espaços — só ilustrativo aqui). Como raw string em Rust:
    let entrada = r#""\"\\\/\b\f\n\r\t""#;

    // Valor esperado, já com os escapes resolvidos: " \ / BS FF LF CR TAB
    let esperado = "\"\\/\u{8}\u{c}\n\r\t";

    assert_eq!(
        brace::parse(entrada).unwrap(),
        Value::String(esperado.to_string())
    );
}

#[test]
fn string_com_escape_unicode_dentro_do_bmp() {
    // \u00e7 é o code point de 'ç' (U+00E7), dentro do BMP: uma única unidade UTF-16.
    let entrada = r#""\u00e7""#;
    assert_eq!(
        brace::parse(entrada).unwrap(),
        Value::String("ç".to_string())
    );
}

#[test]
fn string_com_par_substituto_utf16_completo_fora_do_bmp() {
    // 😀 é U+1F600, fora do BMP. Par substituto UTF-16: alto 0xD83D, baixo 0xDE00.
    let entrada = r#""\uD83D\uDE00""#;
    assert_eq!(
        brace::parse(entrada).unwrap(),
        Value::String("😀".to_string())
    );
}

// ===================================================================
// Valor Unicode literal (não escapado) numa string
// ===================================================================

#[test]
fn string_com_acento_literal_nao_escapado() {
    let entrada = r#""café""#;
    assert_eq!(
        brace::parse(entrada).unwrap(),
        Value::String("café".to_string())
    );
}
