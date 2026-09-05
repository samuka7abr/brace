//! Teste de conformidade: roda `brace::parse` contra a JSONTestSuite
//! vendorizada em `tests/fixtures/JSONTestSuite` (ver `PROVENANCE.md` e
//! `LICENSE` ao lado das fixtures para a origem e a licença da suíte).
//!
//! A JSONTestSuite (https://github.com/nst/JSONTestSuite) é uma coleção de
//! documentos JSON pensada para testar a robustez de parsers contra casos de
//! borda da gramática — não só "isso é JSON válido ou não", mas os limites
//! exatos da RFC 8259 (números com zero à esquerda, escapes Unicode
//! malformados, aninhamento profundo, etc.).
//!
//! A convenção da suíte está codificada no primeiro caractere do nome de
//! cada arquivo, dentro de `test_parsing/`:
//!
//! - `y_*` — JSON válido; um parser correto deve aceitar.
//! - `n_*` — JSON inválido; um parser correto deve rejeitar.
//! - `i_*` — casos definidos pela implementação (limites não fixados pela
//!   RFC, como profundidade de aninhamento ou faixa numérica). Aceitar ou
//!   rejeitar são ambos comportamentos válidos; o único requisito é não
//!   entrar em pânico nem estourar a pilha.
//!
//! # Por que UTF-8 inválido é tratado à parte
//!
//! Vários arquivos `n_*` e `i_*` contêm sequências de bytes que não são
//! UTF-8 válido — isso é proposital, é parte do que a suíte testa. Mas
//! `brace::parse` recebe `&str`, e `&str` em Rust é UTF-8 válido por
//! construção: não existe como entregar bytes inválidos para a função sem
//! primeiro decidir o que fazer com a decodificação.
//!
//! Por isso as fixtures são lidas como bytes crus (`fs::read`) e só depois
//! convertidas com `String::from_utf8`. Essa conversão falhando é, em si, um
//! veredito:
//!
//! - Para um arquivo `n_*`, falha na conversão conta como rejeição
//!   CORRETA — a entrada nunca poderia virar um `&str` para começo de
//!   conversa, então "não vira JSON válido" está automaticamente satisfeito.
//! - Para um arquivo `y_*`, seria uma falha de teste — mas não é esperado
//!   acontecer, já que por definição todo `y_*` é UTF-8 válido.
//!
//! O enum [`Veredito`] modela os três desfechos possíveis e mantém
//! `RejeitadoUtf8` separado de `RejeitadoSintaxe`: são rejeições por motivos
//! diferentes (uma nunca chega ao parser, a outra chega e falha na
//! gramática), e essa distinção precisa aparecer no relatório do teste em
//! vez de ser colapsada.

use std::fs;
use std::path::{Path, PathBuf};

/// Desfecho de avaliar um arquivo de fixture contra `brace::parse`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Veredito {
    /// Os bytes eram UTF-8 válido e `brace::parse` retornou `Ok`.
    Aceito,
    /// Os bytes eram UTF-8 válido, mas `brace::parse` retornou `Err`.
    RejeitadoSintaxe,
    /// Os bytes não eram UTF-8 válido — a entrada nunca chegou ao parser.
    RejeitadoUtf8,
}

/// Caminho absoluto do diretório `test_parsing` da suíte vendorizada.
///
/// Sempre absoluto (via `CARGO_MANIFEST_DIR`), nunca relativo: o diretório de
/// trabalho em que os testes de integração rodam não é garantido pelo cargo.
fn diretorio_fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/JSONTestSuite/test_parsing")
}

/// Lista, em ordem alfabética, os arquivos `.json` de `test_parsing` cujo
/// nome começa com `prefixo` (por exemplo `"y_"`, `"n_"` ou `"i_"`).
///
/// A ordenação existe para o relatório de falhas ser determinístico entre
/// execuções, em vez de depender da ordem que o sistema de arquivos entrega.
fn arquivos_com_prefixo(prefixo: &str) -> Vec<PathBuf> {
    let dir = diretorio_fixtures();
    let entradas = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("não consegui ler o diretório de fixtures {dir:?}: {e}"));

    let mut arquivos: Vec<PathBuf> = entradas
        .map(|entrada| {
            entrada
                .expect("entrada de diretório inválida em test_parsing")
                .path()
        })
        .filter(|caminho| caminho.is_file())
        .filter(|caminho| {
            caminho
                .file_name()
                .and_then(|nome| nome.to_str())
                .is_some_and(|nome| nome.starts_with(prefixo) && nome.ends_with(".json"))
        })
        .collect();

    arquivos.sort();
    arquivos
}

/// Extrai o nome de arquivo (sem o diretório) para usar em mensagens de erro.
fn nome_arquivo(caminho: &Path) -> String {
    caminho
        .file_name()
        .and_then(|nome| nome.to_str())
        .unwrap_or("<nome de arquivo inválido>")
        .to_string()
}

/// Lê `caminho` como bytes crus e avalia o veredito contra `brace::parse`.
///
/// Ver a doc do módulo para por que a decodificação UTF-8 é separada da
/// chamada ao parser em vez de simplesmente usar `fs::read_to_string`.
fn avaliar(caminho: &Path) -> Veredito {
    let bytes = fs::read(caminho).unwrap_or_else(|e| panic!("não consegui ler {caminho:?}: {e}"));

    match String::from_utf8(bytes) {
        Err(_) => Veredito::RejeitadoUtf8,
        Ok(texto) => match brace::parse(&texto) {
            Ok(_) => Veredito::Aceito,
            Err(_) => Veredito::RejeitadoSintaxe,
        },
    }
}

/// Dado um conjunto de mensagens de falha já formatadas, dá `panic!` com a
/// lista inteira se não estiver vazio — assim uma regressão mostra todos os
/// arquivos afetados de uma vez, não só o primeiro em ordem alfabética.
///
/// A lista impressa é limitada a 30 entradas (com um "... e mais N" no fim)
/// para não afogar a saída do teste quando muita coisa quebra de uma vez.
fn falhar_se_houver(falhas: Vec<String>) {
    const LIMITE: usize = 30;

    if falhas.is_empty() {
        return;
    }

    let total = falhas.len();
    let mut relatorio: Vec<String> = falhas.into_iter().take(LIMITE).collect();
    if total > LIMITE {
        relatorio.push(format!("... e mais {} arquivo(s)", total - LIMITE));
    }

    panic!(
        "{total} falha(s) na suíte de conformidade:\n{}",
        relatorio.join("\n")
    );
}

// ===================================================================
// y_*: JSON válido, deve ser aceito
// ===================================================================

#[test]
fn y_devem_ser_aceitos() {
    let mut falhas = Vec::new();

    for caminho in arquivos_com_prefixo("y_") {
        let nome = nome_arquivo(&caminho);
        match avaliar(&caminho) {
            Veredito::Aceito => {}
            Veredito::RejeitadoSintaxe => {
                falhas.push(format!(
                    "{nome}: rejeitado com erro de sintaxe, mas é um caso y_* (deveria ser Ok)"
                ));
            }
            Veredito::RejeitadoUtf8 => {
                falhas.push(format!(
                    "{nome}: bytes não são UTF-8 válido, mas é um caso y_* \
                     (todo y_* deveria ser UTF-8 válido por definição da suíte)"
                ));
            }
        }
    }

    falhar_se_houver(falhas);
}

// ===================================================================
// n_*: JSON inválido, deve ser rejeitado (por sintaxe ou por UTF-8 inválido)
// ===================================================================

#[test]
fn n_devem_ser_rejeitados() {
    let mut falhas = Vec::new();

    for caminho in arquivos_com_prefixo("n_") {
        let nome = nome_arquivo(&caminho);
        if let Veredito::Aceito = avaliar(&caminho) {
            falhas.push(format!(
                "{nome}: aceito como Ok, mas é um caso n_* (deveria ser rejeitado)"
            ));
        }
    }

    falhar_se_houver(falhas);
}

// ===================================================================
// i_*: definido pela implementação — aceitar ou rejeitar valem, mas não
// pode entrar em pânico nem estourar a pilha.
// ===================================================================

#[test]
fn i_nao_podem_quebrar() {
    // O valor deste teste está inteiramente em chegar ao fim do laço sem
    // panic nem estouro de pilha — o veredito de cada arquivo (Aceito,
    // RejeitadoSintaxe ou RejeitadoUtf8) não é verificado, porque qualquer
    // um dos três é um comportamento válido para um caso i_*.
    for caminho in arquivos_com_prefixo("i_") {
        let _ = avaliar(&caminho);
    }
}

// ===================================================================
// Sanidade: a suíte está presente e com a contagem esperada de arquivos.
//
// Sem este teste, um erro de vendoring (diretório vazio, cópia parcial)
// faria os três testes acima passarem vacuamente, iterando zero arquivos.
// ===================================================================

#[test]
fn suite_esta_presente_e_completa() {
    let dir = diretorio_fixtures();
    assert!(
        dir.is_dir(),
        "diretório de fixtures não encontrado: {dir:?} \
         — a JSONTestSuite precisa estar vendorizada em tests/fixtures/JSONTestSuite"
    );

    let quantidade_y = arquivos_com_prefixo("y_").len();
    let quantidade_n = arquivos_com_prefixo("n_").len();
    let quantidade_i = arquivos_com_prefixo("i_").len();
    let total = quantidade_y + quantidade_n + quantidade_i;

    assert_eq!(
        quantidade_y, 95,
        "esperava 95 arquivos y_*, encontrei {quantidade_y} — cópia da suíte incompleta?"
    );
    assert_eq!(
        quantidade_n, 188,
        "esperava 188 arquivos n_*, encontrei {quantidade_n} — cópia da suíte incompleta?"
    );
    assert_eq!(
        quantidade_i, 35,
        "esperava 35 arquivos i_*, encontrei {quantidade_i} — cópia da suíte incompleta?"
    );
    assert_eq!(
        total, 318,
        "esperava 318 arquivos no total, encontrei {total} — cópia da suíte incompleta?"
    );
}
