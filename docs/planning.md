# Planejamento — brace

Roteiro de implementação do parser JSON descrito em `docs/project.md`. Este documento não repete a spec — assume que ela já foi lida — e trata de como chegar lá: ordem de trabalho, decisões técnicas que a spec deixa abertas, e onde estão os riscos reais.

## Estado (2026-09-04)

O plano abaixo foi executado por completo: `src/lib.rs`, `src/value.rs`, `src/error.rs`, `src/lexer.rs`, `src/parser.rs`, `tests/valid.rs` e `tests/invalid.rs` existem e implementam as 7 fases da seção 4. `cargo build` e `cargo clippy --all-targets` passam sem warnings. 113 testes passam: 54 unitários (dentro de `src/`), 35 em `tests/invalid.rs`, 23 em `tests/valid.rs`, mais 1 doctest.

As seções 1 a 8 abaixo continuam sendo o registro técnico válido do raciocínio que guiou a implementação — não foram apagadas. A seção "Questões em aberto" no final foi reescrita: os cinco pontos que estavam abertos foram decididos durante a implementação; o que restou pendente de verdade é só a JSONTestSuite (seção 6), que não foi vendorizada nem integrada.

## 1. Objetivo e critério de pronto

O projeto está pronto quando todos os itens abaixo forem verdadeiros:

- `brace::parse(input: &str) -> Result<Value, ParseError>` existe, é a única função pública de entrada, e cobre a gramática completa descrita no EBNF do `project.md` (objetos, arrays, strings com escape, números inteiro/float/notação científica, `true`/`false`/`null`).
- Todo JSON sintaticamente válido segundo RFC 8259 produz `Ok(Value)` com a árvore correta.
- Toda entrada inválida produz `Err(ParseError)` com `line`/`col` apontando para o caractere ou token onde o problema foi detectado — não uma posição aproximada ou fixa em `(0, 0)`.
- Os módulos `lexer` e `parser` (e qualquer tipo interno de token/posição) são privados ao crate; só `parse`, `Value`, `ParseError` e `ParseErrorKind` são `pub`.
- Existe suíte de testes unitários por módulo, testes de integração via API pública em `tests/`, e pelo menos um subconjunto do JSONTestSuite rodando como referência de conformidade (ver seção 6).
- As limitações assumidas pela spec (precisão de `f64`, `1` vs `1.0`, lookup O(n) em `Object`) estão documentadas no código (doc comments em `Value`/`parse`), não só nesta doc de planejamento.
- `cargo build`, `cargo test` e `cargo clippy` passam sem warnings não justificados.

Não faz parte do critério de pronto: performance benchmarada, serialização, suporte a streaming — isso é explicitamente fora de escopo (seção 8).

## 2. Estado inicial

Registro do ponto de partida, antes da implementação — mantido como histórico. Para o estado atual, ver a seção "Estado" no topo.

Inspecionado diretamente no crate, que naquele momento ficava em `brace/brace/` (depois achatado para a raiz do repositório):

- `Cargo.toml`: crate `brace`, versão `0.1.0`, edition `2024`, sem dependências. Ainda configurado como crate binário (não tem seção `[lib]`, mas isso é implícito pela presença de `src/main.rs`).
- `src/main.rs`: só o template padrão de `cargo init` (`fn main() { println!("Hello, world!"); }`). Nenhuma lógica de parser existe ainda.
- Não há `src/lib.rs`, nem `lexer.rs`, `parser.rs`, `value.rs`, `error.rs`, nem diretório `tests/`.
- `docs/project.md` é a única spec existente. `docs/planning.md` (este arquivo) não existia antes desta sessão.
- Segundo o git status, `brace/` e `docs/` ainda não foram commitados (aparecem como `??` no working tree) — só existe o commit inicial vazio de setup do repositório.

Ou seja: naquele momento, zero código de parser escrito — o trabalho começou do `cargo init` puro.

## 3. Estrutura de arquivos alvo

```
.
├── Cargo.toml
├── docs/
├── src/
│   ├── lib.rs      # ponto de entrada público: parse(), re-exports
│   ├── value.rs    # enum Value (pub)
│   ├── error.rs    # ParseError, ParseErrorKind (pub)
│   ├── lexer.rs    # Lexer, Token/TokenKind (privados ao crate)
│   ├── parser.rs   # Parser, parse_value/parse_object/... (privado ao crate)
│   └── main.rs     # CLI fino de demonstração
└── tests/
    ├── valid.rs
    ├── invalid.rs
    └── fixtures/   # subconjunto do JSONTestSuite, se adotado
```

Responsabilidade de cada módulo:

- **`lib.rs`**: define `pub fn parse(input: &str) -> Result<Value, ParseError>`, que só orquestra `Lexer` → `Vec<Token>` → `Parser` → `Value`. `mod lexer;` e `mod parser;` sem `pub`. Re-exporta `Value`, `ParseError`, `ParseErrorKind` com `pub use`.
- **`value.rs`**: só a definição de `Value` e talvez `impl` triviais (`PartialEq`, `Debug` via derive). Não conhece lexer nem parser.
- **`error.rs`**: `ParseError` e `ParseErrorKind`. Também é o lugar natural para `impl std::fmt::Display for ParseError` e `impl std::error::Error`, mesmo que a spec não peça isso explicitamente — sem isso o erro não compõe bem com `?` fora do crate.
- **`lexer.rs`**: varre `&str` char a char, mantém cursor de linha/coluna, produz a sequência de tokens. Tudo aqui é `pub(crate)` no máximo — nenhum tipo interno do lexer atravessa a borda pública do crate.
- **`parser.rs`**: consome a sequência de tokens recursivamente. Também privado ao crate.

## 4. Fases de implementação

A ordem segue a dependência natural do pipeline: não dá para escrever o parser sem os tokens, não dá para testar o lexer sem os tipos base. Cada fase produz algo testável isoladamente antes de avançar.

**Fase 1 — Tipos base.** `Value`, `Token`/`TokenKind`, `ParseError`, `ParseErrorKind`, e a conversão do crate para lib (`src/lib.rs` existindo, `Cargo.toml` ajustado se necessário). Nenhum comportamento ainda. Testável: os tipos compilam, derivam `Debug`/`PartialEq`/`Clone` onde fizer sentido, e um teste unitário trivial consegue construir um `Value::Object(vec![...])` à mão.

**Fase 2 — Lexer de estruturais e literais.** Reconhece `{ } [ ] : ,`, as palavras-chave `true`/`false`/`null`, espaços em branco (ignorados) e `Eof`. Testável: dado `"{ }"` ou `"[true, false, null]"`, o lexer produz a sequência de tokens esperada; caractere desconhecido produz `UnexpectedChar`.

**Fase 3 — Lexer de strings com escapes.** Reconhece `"..."`, trata `\" \\ \/ \b \f \n \r \t`, e o caso difícil: `\uXXXX` com pares substitutos UTF-16 (seção 5a). Testável: strings simples, strings com cada escape simples, string com um caractere fora do BMP via par substituto, string com aspas não fechada (`UnterminatedString`), escape inválido (`InvalidEscape`).

**Fase 4 — Lexer de números.** Valida a gramática manualmente antes de repassar para `str::parse::<f64>()` (seção 5b). Testável: inteiros, negativos, frações, notação científica com `e`/`E` e sinal, e rejeição de formas inválidas (`01`, `1.`, `.5`, `+1`, `1e`).

**Fase 5 — Parser recursivo descendente.** Com o lexer completo e testado, escrever `parse_value`, `parse_object`, `parse_array`, delegando strings/números/literais direto do token. Testável: documentos JSON completos e válidos (aninhados, com objetos dentro de arrays etc.) virando `Value` corretos; erros de gramática (`,` sobrando, `:` faltando, chave que não é string) produzindo `UnexpectedToken`.

**Fase 6 — Linha/coluna de ponta a ponta.** Nas fases 2–5 a posição pode ter sido tratada de forma incompleta ou provisória; esta fase fecha a lacuna: garantir que todo token carregue sua posição de início, que o parser recupere essa posição ao gerar `ParseError`, e que os testes de erro passem a asserir `line`/`col` exatos, não só o `kind`. Ver seção 5c para o porquê disso ser uma fase separada em vez de "já ter saído certo" nas fases anteriores.

**Fase 7 — API pública e polimento.** Fechar a superfície pública (só `parse`, `Value`, `ParseError`, `ParseErrorKind` são `pub`), decidir o destino de `src/main.rs`, revisar `clippy`, escrever doc comments com as limitações conhecidas, e rodar o subconjunto do JSONTestSuite escolhido.

Ordem alternativa considerada e rejeitada: escrever o parser antes do lexer de números/strings completo, usando stubs. Não compensa — o parser depende inteiramente da forma exata dos tokens que os itens 3 e 4 definem (em especial, se erros de lexer de string/número abortam o lexing inteiro ou geram um token de erro), então adiar 3 e 4 só empurra retrabalho para depois.

## 5. Pontos de decisão técnica

### 5a. Escapes de string, `\uXXXX` e pares substitutos

O ponto mais fácil de acertar errado. JSON permite `\uXXXX` onde `XXXX` é uma *code unit* UTF-16, não um code point. A maioria dos caracteres cabe em uma única `\uXXXX`, mas qualquer coisa fora do BMP (emojis, por exemplo) chega como um par substituto: uma code unit "alta" (`0xD800`–`0xDBFF`) seguida de uma "baixa" (`0xDC00`–`0xDFFF`).

Isso importa porque `String` em Rust é UTF-8 e cada `char` é um *Unicode scalar value* — não existe `char` para uma code unit substituta isolada. Então:

- Ao encontrar `\uXXXX` com `XXXX` na faixa alta, o lexer precisa *esperar* o próximo escape: se não vier `\uYYYY` com `YYYY` na faixa baixa, é entrada inválida (substituto solto).
- Combinar o par: `codepoint = 0x10000 + (alto - 0xD800) * 0x400 + (baixo - 0xDC00)`, depois `char::from_u32(codepoint)`.
- Uma code unit baixa aparecendo sem uma alta antes também é inválida, pelo mesmo motivo.

`ParseErrorKind` como está na spec não tem uma variante dedicada para "substituto solto/incompleto" — só `InvalidEscape(char)`, que assume um caractere de escape ruim, não um `\u` sintaticamente válido mas semanticamente incompleto. Decisão registrada em "Questões em aberto".

### 5b. Número sem crate: validar a gramática à mão

A tentação é chamar `str::parse::<f64>()` direto na substring e deixar ele decidir se é válido. Problema: o parser de float da std é mais permissivo que JSON (aceita `inf`, `NaN`, `1_000`, notação sem dígito antes do ponto em algumas variações) e mais estrito em outras (não aceita `+1`). Confiar nele decide silenciosamente o dialeto do `brace`.

A abordagem correta: o lexer varre a gramática manualmente, aceitando exatamente:

```
"-"?  ("0" | dígito(1-9) dígito*)  ("." dígito+)?  (("e"|"E") ("+"|"-")? dígito+)?
```

Note que isso é mais estrito que o EBNF simplificado do `project.md` (`digit+` para a parte inteira, sem a regra de zero-líder). RFC 8259 proíbe `01` como número (exceto o próprio `0`); é essa regra estrita que a JSONTestSuite testa em `n_number_*`. Recomendo seguir o RFC completo aqui, não o EBNF simplificado — ver "Questões em aberto".

Só depois de reconhecer essa forma exata (avançando o cursor caractere a caractere e guardando os limites da substring) é que se chama `str::parse::<f64>()` sobre o trecho validado. Nesse ponto o parse não deveria falhar nunca — se falhar, é bug do validador, não entrada inválida do usuário, então vale um `debug_assert!` ou similar em vez de silenciosamente mapear para `InvalidNumber`.

Caso de borda a decidir: um número sintaticamente válido mas fora do alcance de `f64` (ex.: `1e400`) resulta em `f64::INFINITY` via `parse`. A gramática não proíbe isso, e RFC 8259 permite que implementações imponham limites próprios. Decisão pragmática: aceitar (`Ok` com `Infinity`) e não tratar como erro — é consistente com "não apanhar num Number com tipo próprio", que já foi descartado pela spec.

### 5c. Onde o cursor de linha/coluna é atualizado, e como o erro recupera a posição

Dois cuidados separados aqui.

**Avanço do cursor**: precisa acontecer num único ponto do lexer — a função que consome um `char` por vez (algo como `fn advance(&mut self) -> Option<char>`) — nunca espalhado pelos métodos de tokenização. A cada `char` consumido: se for `'\n'`, `line += 1; col = 1`; caso contrário, `col += 1`. Isso implica iterar por `char`, não por byte (`str::chars()` ou `char_indices()`), porque a entrada é UTF-8 e um caractere multi-byte não deve contar como várias colunas. Convenção adotada: coluna conta *caracteres Unicode* (scalar values), não bytes nem code units UTF-16 — mais simples de implementar e suficiente para apontar erro num editor de texto comum.

**Recuperação da posição pelo parser**: aqui a spec deixa uma lacuna real. O `Token` como listado no `project.md` (`LBrace`, `String(String)`, `Number(f64)`, etc.) não carrega posição nenhuma. Se o `Vec<Token>` produzido pelo lexer for literalmente isso, o parser não tem como saber onde cada token começou depois que o lexer já terminou — a única informação que sobra é a posição *atual* do parser, que não corresponde a nada.

Decisão: o que a spec chama de "Token" na seção de estruturas de dados vira, na implementação, `TokenKind` (o enum sem mudanças), envolto por um `Token` (ou `Spanned<TokenKind>`) que carrega `line`/`col` do início daquele token:

```rust
struct Token {
    kind: TokenKind,
    line: usize,
    col: usize,
}
```

O pipeline documentado (`&str -> Lexer -> Vec<Token> -> Parser -> Value`) continua correto — só que agora "Token" já é o tipo posicionado. Isso é o que permite ao parser, ao encontrar um token inesperado, copiar `line`/`col` direto dele para o `ParseError`, sem manter nenhum estado de posição próprio. Para `UnexpectedEof`, o lexer sempre emite um token `Eof` final com a posição logo após o último caractere consumido, e o parser usa essa posição normalmente — sem caso especial.

### 5d. Profundidade de recursão em JSON aninhado

Um parser recursivo descendente empilha um frame de `parse_value` por nível de aninhamento. Um JSON adversarial como `[[[[[...]]]]]` com milhares de níveis pode estourar a pilha antes de qualquer erro de sintaxe ser detectado — isso é um crash do processo, não um `Err` tratável, e é justamente o tipo de entrada que a JSONTestSuite testa (fixtures de profundidade estrutural, prefixo `i_` e `n_structure_100000_opening_arrays` e similares).

RFC 8259 permite explicitamente que implementações imponham um limite de aninhamento. Decisão recomendada: manter um contador de profundidade no parser, incrementado a cada entrada em `parse_object`/`parse_array` e decrementado ao sair, com um limite conservador (por exemplo 128 ou 256 — não crítico, só precisa ser bem menor que o que estoura a pilha real da máquina). Ultrapassar o limite gera erro tratável em vez de crash.

Isso exige uma variante de erro que a spec não lista (`ParseErrorKind` atual não tem nada equivalente a "profundidade máxima excedida"). Registrado em "Questões em aberto".

### 5e. Tratamento de EOF

Dois pontos:

- Entrada vazia (`""`) deve resultar em erro (`UnexpectedEof`), não em pânico nem em nenhum `Value` default — RFC 8259 não define texto JSON vazio como válido.
- Depois que `parse_value` no topo consome um valor completo, é preciso verificar que só resta o token `Eof` — se houver qualquer coisa depois (`"true" "false"` concatenados, por exemplo), isso é erro (`UnexpectedToken`), não deve ser silenciosamente ignorado. Fácil de esquecer porque o parser "termina com sucesso" ao reconhecer o primeiro valor válido se essa checagem final não existir.

## 6. Estratégia de testes

**Unitários por módulo**: cada arquivo (`lexer.rs`, `parser.rs`, `error.rs`) com seu próprio `#[cfg(test)] mod tests`, testando o tipo/função daquele módulo isoladamente — por exemplo, o lexer é testado produzindo `Vec<Token>` diretamente, sem passar pelo parser.

**Integração via API pública**: `tests/valid.rs` e `tests/invalid.rs` chamando só `brace::parse`, nunca os módulos internos (eles são privados ao crate, então isso nem compilaria de fora). É o que garante que a API pública funciona fim a fim, não só as peças internas.

**Casos de erro com asserção de posição**: não basta `assert!(result.is_err())`. Cada teste de entrada inválida deve fixar a entrada exata e asserir `line`/`col` esperados, por exemplo:

```rust
let err = brace::parse("{\n  \"a\": ,\n}").unwrap_err();
assert_eq!((err.line, err.col), (2, 8));
```

Um helper de teste que recebe `(input, expected_line, expected_col, expected_kind)` evita repetir esse boilerplate a cada caso.

**JSONTestSuite como referência de conformidade**: o repositório [seriot/JSONTestSuite](https://github.com/nst/JSONTestSuite) tem arquivos `.json` prefixados por categoria:

- `y_*`: devem ser aceitos (`parse` retorna `Ok`).
- `n_*`: devem ser rejeitados (`parse` retorna `Err`).
- `i_*`: comportamento implementation-defined (números fora de alcance, substitutos soltos, aninhamento extremo) — não há resposta "certa" única; o único requisito razoável é não entrar em pânico nem estourar pilha.

Sugestão de harness: um teste de integração que itera um diretório de fixtures vendorizado (subconjunto escolhido, não necessariamente a suíte inteira — ela tem centenas de arquivos) e aplica a regra acima por prefixo do nome do arquivo. Vendorizar via cópia direta de uma seleção de arquivos é mais simples de manter do que submódulo git para um projeto deste tamanho; decisão de como trazer os arquivos para o repo fica para quando essa fase começar.

## 7. Riscos e armadilhas conhecidas

A spec já admite três limitações. Vale nomear a consequência prática de cada uma, não só repetir que existem:

- **`Number` como `f64` perde precisão acima de 2^53.** Um inteiro como `9007199254741897` (ID, timestamp em nanossegundos, etc.) volta arredondado, sem aviso — `parse` não retorna erro, só devolve um valor numericamente diferente do que estava no texto. Qualquer consumidor que passe esse tipo de dado por `brace` está exposto a corrupção silenciosa. Precisa estar em destaque na doc pública, não só nesta seção.
- **`1` e `1.0` colapsam no mesmo `Value::Number(1.0)`.** Perde-se, no momento do lexing, a informação de qual forma o texto original usava. Isso é irreversível depois de tokenizado — se um dia `brace` precisar de round-trip (serialização fiel ao original, hoje fora de escopo), essa perda já terá acontecido e não tem como recuperar sem re-lexar o texto original.
- **`Object` como `Vec<(String, Value)>` é O(n) por chave buscada.** Aceitável para o objetivo do projeto (preservar ordem, documentos pequenos/médios). Não seria aceitável se algum consumidor fizer lookup repetido em objetos com milhares de chaves — nesse caso o custo é quadrático em laços que buscam chave por chave.

Riscos adicionais que a spec não menciona:

- **Chaves duplicadas em objeto** (`{"a": 1, "a": 2}`) — RFC 8259 deixa esse caso como comportamento não especificado. `Vec<(String, Value)>` naturalmente mantém as duas entradas, sem dedup automático. Isso diverge do comportamento comum de bibliotecas como `serde_json` (que mantém só a última). Precisa ser uma escolha documentada, não um acidente de implementação.
- **Estouro de pilha em aninhamento profundo** — coberto em 5d como decisão técnica, mas é também um risco de robustez: sem limite, uma entrada adversarial pequena em bytes (`"[" * 100000`) derruba o processo.
- **Substitutos UTF-16 soltos ou incompletos** em `\uXXXX` — coberto em 5a; risco aqui é implementar o caminho feliz (par completo) e esquecer os dois casos de erro (alto sem baixo, baixo sem alto), que só aparecem em fixtures de teste adversariais, não em JSON do dia a dia.

## 8. Fora de escopo

Reafirmando o que já está em `project.md`, sem adicionar nada:

- JSON5/JSONC (comentários, trailing comma, chave sem aspas).
- Parsing incremental/streaming.
- Serialização (`Value` → string).
- Precisão arbitrária em números.

## Questões em aberto

Dos cinco pontos que este plano deixava em aberto, todos os cinco foram decididos durante a implementação. Registro de cada decisão, e o que continua realmente pendente no final.

1. **`Token` sem posição — decidido: envolver com posição.** O enum `Token` do `project.md` virou `TokenKind`, envolto por um `Token { kind, line, col }` que carrega a posição de início de cada token. Implementado como planejado na seção 5c; é uma extensão da spec, não uma leitura literal dela, mas necessária para o requisito de erro com posição exata.
2. **`ParseErrorKind` incompleto para os casos novos — decidido: adicionar variantes novas, não reaproveitar.** `InvalidUnicodeEscape` cobre o par substituto UTF-16 solto ou incompleto em `\uXXXX`; `DepthLimitExceeded` cobre a profundidade máxima excedida. Nenhuma das duas reaproveita `InvalidEscape`/`UnexpectedEof` — a mensagem de erro fica específica para cada caso, ao custo de duas variantes extras no enum público.
3. **Gramática de número: EBNF simplificado vs. RFC 8259 estrito — decidido: RFC 8259 estrito.** A implementação segue o RFC completo, não o EBNF simplificado do `project.md`. Na prática isso é **mais restritivo** do que a spec escrita ao pé da letra: `01`, `1.`, `.5`, `+1`, `1e` e `1e+` são todos `InvalidNumber`, quando o EBNF simplificado (`digit+` na parte inteira) permitiria `01`. Essa é a decisão com maior distância da spec literal, junto com a variante `2` abaixo — vale confirmação do autor se o comportamento pretendido for de fato o mais permissivo.
4. **Comportamento em chaves duplicadas — decidido: manter todas as ocorrências.** `{"a": 1, "a": 2}` produz `Value::Object` com as duas entradas, sem dedup — coerente com `Vec` em vez de `HashMap`, e documentado no doc comment de `Value`.
5. **Destino de `src/main.rs` — decidido: manter como binário fino de demonstração.** Lê um arquivo (primeiro argumento) ou stdin, chama `parse`, imprime `{:#?}` do `Value` ou a mensagem de erro. Não removido.

Um ponto adicional, não listado nas cinco questões originais, também foi resolvido durante a implementação: `UnexpectedToken.found` é `&'static str`, não `Token` como o `project.md` define literalmente — motivo técnico, não preferência. `Token`/`TokenKind` são `pub(crate)` (privados ao crate, ver questão 1 e a decisão de fronteira pub/privado) e `ParseErrorKind` é público; um tipo privado não pode aparecer num campo de um tipo público em Rust. O `&'static str` carrega a descrição do token, produzida por `TokenKind::describe()`. É, junto com a questão 3, o desvio mais literal da spec escrita entre as decisões tomadas.

O que continua pendente de verdade: a JSONTestSuite (seção 6) não foi vendorizada nem integrada ao repositório. Não há harness, não há fixtures, não há teste de conformidade correspondente — isso é trabalho futuro, não uma decisão tomada.
