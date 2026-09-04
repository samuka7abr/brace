//! Binário fino de demonstração do `brace`: lê JSON de um arquivo (primeiro
//! argumento) ou de stdin, parseia e imprime o resultado.

use std::env;
use std::fs;
use std::io::{self, Read};
use std::process::ExitCode;

fn main() -> ExitCode {
    let entrada = match ler_entrada() {
        Ok(texto) => texto,
        Err(e) => {
            eprintln!("erro ao ler entrada: {e}");
            return ExitCode::FAILURE;
        }
    };

    match brace::parse(&entrada) {
        Ok(valor) => {
            println!("{valor:#?}");
            ExitCode::SUCCESS
        }
        Err(erro) => {
            eprintln!("{erro}");
            ExitCode::FAILURE
        }
    }
}

/// Lê o conteúdo do arquivo passado como primeiro argumento, ou de stdin
/// se nenhum argumento foi dado.
fn ler_entrada() -> io::Result<String> {
    match env::args().nth(1) {
        Some(caminho) => fs::read_to_string(caminho),
        None => {
            let mut buffer = String::new();
            io::stdin().read_to_string(&mut buffer)?;
            Ok(buffer)
        }
    }
}
