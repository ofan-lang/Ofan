use clap::Parser;
use std::path::PathBuf;

mod ast;
mod codegen;
mod lexer;
mod parser;
mod typechecker;

#[derive(Parser)]
#[command(name = "ofan", about = "The Ofan language compiler")]
struct Args {
    /// Source file to compile (.ofn)
    source: PathBuf,
}

fn main() {
    let args = Args::parse();

    let source = match std::fs::read_to_string(&args.source) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("ofan: cannot read '{}': {e}", args.source.display());
            std::process::exit(1);
        }
    };

    let tokens = match lexer::Lexer::new(&source).lex() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("ofan: lex error: {e}");
            std::process::exit(1);
        }
    };

    let ast = match parser::Parser::new(tokens).parse() {
        Ok(ast) => ast,
        Err(e) => {
            eprintln!("ofan: parse error: {e}");
            std::process::exit(1);
        }
    };

    match typechecker::infer(&ast) {
        Ok(result) => {
            if result.has_deferred() {
                for d in &result.deferred {
                    eprintln!("ofan: unsupported construct: {d}");
                }
                eprintln!("ofan: cannot compile: source contains unresolved constructs");
                std::process::exit(1);
            }
            #[cfg(feature = "codegen")]
            {
                use codegen::llvm::LlvmContext;
                let stem = args.source.file_stem().unwrap_or_default();
                let out = args.source.with_file_name(stem)
                    .with_extension(std::env::consts::EXE_EXTENSION);
                let ctx = LlvmContext::new();
                if let Err(e) = ctx.emit_hardcoded_main(&out) {
                    eprintln!("ofan: codegen error: {e}");
                    std::process::exit(1);
                }
                eprintln!("ofan: compiled \u{2192} {}", out.display());
            }
            #[cfg(not(feature = "codegen"))]
            {
                eprintln!(
                    "ofan: codegen not yet implemented\nsource: {}",
                    args.source.display()
                );
                std::process::exit(1);
            }
        }
        Err(errors) => {
            for e in &errors {
                eprintln!("ofan: type error: {e}");
            }
            std::process::exit(1);
        }
    }
}
