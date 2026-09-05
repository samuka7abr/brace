# brace 

**Parser de JSON (RFC 8259) escrito do zero, sem `serde_json`, `nom` ou `pest`.** Todo erro de sintaxe volta com a linha e a coluna exatas de onde a entrada quebrou.

<img src="https://raw.githubusercontent.com/TheZoq2/ferris/master/rustacean-flat-happy.svg" height="60"/>

---

## Overview

`brace` é um parser de JSON (RFC 8259) implementado em Rust puro. Não depende de nenhuma crate de parsing — o `Cargo.toml` não tem uma única linha em `[dependencies]`, só a std. Recebe uma string de entrada e devolve uma árvore `Value` em memória, ou um `ParseError` que aponta exatamente onde a sintaxe quebrou.

## Arquitetura

Duas fases, como qualquer parser recursivo descendente:

```
&str ──> [Lexer] ──> Vec<Token> ──> [Parser] ──> Value
```

O `Lexer` varre a entrada caractere a caractere e emite tokens; não conhece gramática, só reconhece unidades léxicas (`{`, `"chave"`, `42`). O `Parser` consome esses tokens recursivamente, um método por regra da gramática.

A decisão não óbvia está no tipo `Token`. A spec original descrevia um enum simples (`LBrace`, `String(String)`, `Number(f64)`, ...) sem nenhuma posição. Só que o lexer termina de rodar antes do parser começar — se o token não carrega onde começou, essa informação já foi perdida quando um erro de gramática aparece depois. A solução foi separar o conteúdo do token (`TokenKind`) da sua posição: `Token` passou a ser um `TokenKind` envolto com `line`/`col` do primeiro caractere daquele token. Isso é o que permite ao parser, ao encontrar um token inesperado, copiar a posição direto do token para dentro do `ParseError` — ele nunca precisa manter um cursor de posição próprio.

## Uso

```rust
use brace::{parse, Value};

let valor = parse(r#"{"nome": "brace", "versao": 1}"#)?;

match valor {
    Value::Object(pares) => assert_eq!(pares.len(), 2),
    _ => unreachable!(),
}
```

Também há um binário fino de demonstração, que lê de um arquivo ou de stdin:

```bash
echo '{"a": [1, true, null]}' | cargo run
cargo run -- arquivo.json
```

## Erros com posição

É o diferencial do projeto. Toda entrada inválida produz um `ParseError` com linha e coluna exatas, contadas a partir de 1, não uma posição aproximada.

```rust
let erro = brace::parse("{\n  \"a\": ,\n}").unwrap_err();
println!("{erro}");
```

Saída:

```text
erro de sintaxe na linha 2, coluna 8: esperava valor, encontrou ","
```

`ParseErrorKind` tem oito variantes:

| Variante | Quando ocorre |
|---|---|
| `UnexpectedChar` | Caractere que não inicia nenhum token válido. |
| `UnexpectedToken` | A gramática esperava um token e encontrou outro. |
| `UnterminatedString` | String aberta com `"` que nunca fecha (inclui EOF no meio de um escape). |
| `InvalidEscape` | Sequência `\X` com `X` que não é um escape reconhecido. |
| `InvalidNumber` | Número que não segue a gramática de `number` do RFC 8259. |
| `UnexpectedEof` | Entrada terminou onde um token era esperado. |
| `InvalidUnicodeEscape` | `\uXXXX` com par substituto UTF-16 solto ou incompleto. |
| `DepthLimitExceeded` | Aninhamento de objetos/arrays acima do limite permitido. |

## Conformidade

O parser segue o RFC 8259 de forma estrita, o que em alguns pontos é mais restritivo do que a leitura mais óbvia da gramática:

- Rejeita formas que `str::parse::<f64>()` aceitaria mas o RFC proíbe: `01`, `1.`, `.5`, `+1`, `1e`. Todas retornam `InvalidNumber`.
- Rejeita form feed, vertical tab e BOM como espaço em branco entre tokens. JSON só admite espaço, tab, `\n` e `\r` — qualquer outro caractere aí é `UnexpectedChar`.
- Aceita `\"` escapado dentro de string, mas rejeita newline e tab literais no meio de uma string (RFC 8259 proíbe caracteres de controle literais em string; a forma correta é escapá-los).
- Combina pares substitutos UTF-16 corretamente: `"😀"` vira `"😀"`. Um substituto solto ou incompleto é `InvalidUnicodeEscape`, nunca cai silenciosamente em `U+FFFD`.
- Impõe limite de profundidade de aninhamento de 128 níveis. Um documento com 100.000 colchetes abertos em sequência retorna `DepthLimitExceeded` em vez de estourar a pilha de chamadas.

A JSONTestSuite ([nst/JSONTestSuite](https://github.com/nst/JSONTestSuite)) foi vendorizada e roda em `cargo test`. As fixtures estão em `tests/fixtures/JSONTestSuite/test_parsing/` (318 arquivos: 95 `y_`, 188 `n_`, 35 `i_`, com a `LICENSE` MIT original e um `PROVENANCE.md` descrevendo origem e convenção de nomes), e o harness é `tests/conformance.rs`. Resultado: 95/95 `y_` aceitos, 188/188 `n_` rejeitados, 35 `i_` sem quebrar — zero crashes e zero timeouts nos 318 arquivos. Nuance: 12 dos 188 `n_` não são rejeitados pela lógica de parsing em si — contêm UTF-8 inválido e são barrados na conversão para `String`, antes de `brace::parse` ser chamado; o veredito final está correto, mas o mérito é do tipo `&str` do Rust, não do parser. Os outros 176 `n_` são rejeitados por sintaxe de verdade; o harness distingue os dois casos com um enum `Veredito` de três estados.

## Limitações conhecidas

- **`Number` é `f64`.** Perde precisão em inteiros acima de 2^53: `9007199254740993` volta como `9007199254740992`, sem erro sinalizado. É perda silenciosa, assumida desde a spec — não é um bug a corrigir.
- **`1` e `1.0` colapsam no mesmo `Value::Number(1.0)`.** A informação de qual forma o texto original usava se perde no lexing e não tem como ser recuperada depois.
- **`Object` é `Vec<(String, Value)>`.** Preserva a ordem de inserção das chaves, mas lookup é O(n) em vez de O(1). Chaves duplicadas (`{"a":1,"a":2}`) são mantidas as duas, sem dedup — RFC 8259 deixa esse caso como não especificado, e essa é a escolha deliberada do projeto.
- **O limite de 128 níveis de aninhamento rejeita JSON válido, porém muito profundo.** É uma escolha consciente para não estourar a pilha real, explicitamente permitida pelo RFC 8259 (que autoriza limites de implementação), não uma consequência acidental.
- **Fora de escopo**: JSON5/JSONC (comentários, trailing comma, chave sem aspas), parsing incremental/streaming, serialização (`Value` → string), precisão arbitrária em números.
- **`test_transform/` da JSONTestSuite não é coberto.** Esse diretório da suíte testa como transformar valores ambíguos (número fora de alcance, chave duplicada), um eixo diferente do que `test_parsing/` cobre (aceitar/rejeitar). Não vendorizado, não testado.

## Qualidade

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Estado atual: 117 testes passando — 54 unitários (dentro de `src/`), 4 de conformidade em `tests/conformance.rs`, 35 em `tests/invalid.rs`, 23 em `tests/valid.rs`, mais 1 doctest. Clippy sem warnings.

## Estrutura do projeto

```text
.
├── src/
│   ├── lib.rs      # ponto de entrada público: parse(), re-exports
│   ├── value.rs    # enum Value
│   ├── error.rs    # ParseError, ParseErrorKind
│   ├── lexer.rs    # Lexer, Token/TokenKind (privados ao crate)
│   ├── parser.rs   # Parser recursivo descendente (privado ao crate)
│   └── main.rs     # binário fino de demonstração (arquivo ou stdin)
├── tests/
│   ├── valid.rs         # integração via API pública: entradas que devem parsear
│   ├── invalid.rs       # integração via API pública: entradas que devem falhar, com posição
│   ├── conformance.rs   # harness da JSONTestSuite vendorizada: y_/n_/i_ por prefixo
│   └── fixtures/        # JSONTestSuite vendorizada (test_parsing/, LICENSE, PROVENANCE.md)
└── docs/
    ├── project.md    # spec: gramática, estruturas de dados, escopo
    └── planning.md   # roteiro de implementação, decisões técnicas e estado
```
