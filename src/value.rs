//! Tipo de dado central do crate: a árvore de valores JSON em memória.

/// Representa qualquer valor JSON válido (RFC 8259) já parseado.
///
/// # Limitações conhecidas
///
/// - **`Object` como `Vec<(String, Value)>`**: preserva a ordem de inserção
///   das chaves (útil para reproduzir o JSON original), mas o custo é
///   lookup O(n) por chave em vez de O(1) como um `HashMap` teria. Além
///   disso, chaves duplicadas (`{"a": 1, "a": 2}`) **não** passam por
///   dedup — ambas as entradas são mantidas na ordem em que apareceram.
///   RFC 8259 deixa esse caso como comportamento não especificado; esta é
///   uma escolha deliberada, não um acidente de implementação.
/// - **`Number` como `f64`**: mais simples de implementar, mas perde
///   precisão em inteiros acima de 2^53 (um ID ou timestamp em
///   nanossegundos pode voltar arredondado, sem qualquer erro sinalizado)
///   e não distingue `1` de `1.0` — a informação de qual forma o texto
///   original usava se perde no momento do lexing e não pode ser
///   recuperada depois.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// O literal `null`.
    Null,
    /// Os literais `true` e `false`.
    Bool(bool),
    /// Qualquer número JSON (inteiro, float ou notação científica), guardado como `f64`.
    Number(f64),
    /// Uma string JSON, já com os escapes resolvidos.
    String(String),
    /// Uma lista ordenada de valores.
    Array(Vec<Value>),
    /// Um objeto: lista ordenada de pares chave/valor, sem dedup de chaves.
    Object(Vec<(String, Value)>),
}

#[cfg(test)]
mod tests {
    use super::Value;

    #[test]
    fn constroi_valor_aninhado() {
        let valor = Value::Object(vec![
            ("nome".to_string(), Value::String("brace".to_string())),
            (
                "tags".to_string(),
                Value::Array(vec![
                    Value::String("json".to_string()),
                    Value::Number(1.0),
                    Value::Bool(true),
                    Value::Null,
                ]),
            ),
        ]);

        let esperado = Value::Object(vec![
            ("nome".to_string(), Value::String("brace".to_string())),
            (
                "tags".to_string(),
                Value::Array(vec![
                    Value::String("json".to_string()),
                    Value::Number(1.0),
                    Value::Bool(true),
                    Value::Null,
                ]),
            ),
        ]);

        assert_eq!(valor, esperado);
    }

    #[test]
    fn chaves_duplicadas_sao_mantidas_sem_dedup() {
        let valor = Value::Object(vec![
            ("a".to_string(), Value::Number(1.0)),
            ("a".to_string(), Value::Number(2.0)),
        ]);

        match valor {
            Value::Object(pares) => assert_eq!(pares.len(), 2),
            _ => unreachable!(),
        }
    }
}
