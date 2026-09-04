# brace

## O que é

Parser de JSON (RFC 8259) escrito do zero em Rust, sem crates de parsing (`serde_json`, `pest`, `nom`, etc). Recebe uma string de entrada e produz uma árvore de valores em memória (`Value`) ou um erro de sintaxe com posição.

## O que deve fazer

- Ler um texto JSON válido e transformá-lo numa estrutura `Value` navegável em memória.
- Rejeitar entrada inválida com um erro que aponte linha e coluna do problema, não só "parse failed".
- Cobrir a gramática completa: objetos, arrays, strings com escapes, números (inteiro, float, notação científica), `true`/`false`/`null`.

## Arquitetura

Duas fases, como todo parser recursivo descendente:

```
&str ──> [Lexer] ──> Vec<Token> ──> [Parser] ──> Value
```

**Lexer**: varre a string caractere a caractere e emite tokens. Não conhece gramática, só reconhece unidades léxicas (`{`, `"chave"`, `42`, etc).

**Parser**: consome os tokens recursivamente, um método por regra da gramática (`parse_value`, `parse_object`, `parse_array`...). Cada método sabe qual token esperar em seguida e chama o próximo nível quando encontra `{`, `[` ou um literal.

## Gramática (EBNF simplificado)

```
value   = object | array | string | number | "true" | "false" | "null"
object  = "{" [ member ("," member)* ] "}"
member  = string ":" value
array   = "[" [ value ("," value)* ] "]"
string  = '"' char* '"'
number  = "-"? digit+ ("." digit+)? (("e"|"E") ("+"|"-")? digit+)?
```

## Estruturas de dados

```rust
pub enum Value {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Value>),
    Object(Vec<(String, Value)>),
}
```

`Object` como `Vec<(String, Value)>` em vez de `HashMap<String, Value>`: preserva a ordem de inserção das chaves (útil pra reproduzir o JSON original) ao custo de lookup O(n) em vez de O(1). Pra esse projeto, ordem > performance de busca — não é uma estrutura pensada pra consultas frequentes.

`Number` como `f64`: mais simples, mas perde precisão em inteiros muito grandes (acima de 2^53) e não distingue `1` de `1.0`. É uma limitação conhecida, não um bug — resolver isso direito exigiria um tipo `Number` próprio guardando a representação original.

Tokens do lexer:

```rust
enum Token {
    LBrace, RBrace,   // { }
    LBracket, RBracket, // [ ]
    Colon, Comma,
    String(String),
    Number(f64),
    True, False, Null,
    Eof,
}
```

## Tratamento de erro

```rust
pub struct ParseError {
    pub kind: ParseErrorKind,
    pub line: usize,
    pub col: usize,
}

pub enum ParseErrorKind {
    UnexpectedChar(char),
    UnexpectedToken { expected: &'static str, found: Token },
    UnterminatedString,
    InvalidEscape(char),
    InvalidNumber,
    UnexpectedEof,
}
```

Lexer e parser mantêm um cursor de linha/coluna atualizado a cada caractere consumido, pra todo erro carregar posição exata.

## Interface pública

```rust
pub fn parse(input: &str) -> Result<Value, ParseError>;
```

Uma função só como ponto de entrada. Lexer e parser ficam privados ao crate — o consumidor não precisa saber que existem duas fases.

## Fora de escopo

- Extensões tipo JSON5/JSONC (comentários, trailing comma, chave sem aspas).
- Parsing incremental/streaming (entrada inteira precisa estar em memória).
- Serialização (`Value` → string JSON) — pode virar um `impl Display` depois, mas não é o objetivo inicial.
- Precisão arbitrária em números.
