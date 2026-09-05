# JSONTestSuite

Arquivos de teste copiados de https://github.com/nst/JSONTestSuite (licença MIT,
preservada em `LICENSE`), da suíte que acompanha o artigo "Parsing JSON is a
Minefield", de Nicolas Seriot.

Somente o diretório `test_parsing/` foi copiado. O harness que consome esses
arquivos é `tests/conformance.rs`.

Convenção dos nomes, aplicada pelo harness:

- `y_*` — JSON válido, deve ser aceito.
- `n_*` — JSON inválido, deve ser rejeitado.
- `i_*` — comportamento definido pela implementação; aceitar ou rejeitar é
  válido, quebrar não é.
