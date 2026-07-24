use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

use ofan::{ast, lexer, parser, typechecker};

// ─── CLI definition ───────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "ofan", about = "The Ofan language compiler")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Compile a .ofn source file to a native binary.
    Build {
        /// Source file to compile (.ofn)
        source: PathBuf,
        /// Output path for the compiled binary [default: ./<stem>[.exe]]
        #[arg(short = 'o', long = "output")]
        output: Option<PathBuf>,
    },
    /// Compile a .ofn source file and immediately execute it.
    ///
    /// Exits with the compiled program's exit code. Pass arguments to the
    /// program after `--`: `ofan run foo.ofn -- arg1 arg2`
    Run {
        /// Source file to compile and run (.ofn)
        source: PathBuf,
        /// Arguments forwarded to the compiled program
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// Type-check a .ofn source file without invoking the code generator.
    Check {
        /// Source file to type-check (.ofn)
        source: PathBuf,
    },
}

// ─── Entry point ──────────────────────────────────────────────────────────────

fn main() {
    // Run all logic in a separate function so locals (including the TempBinary
    // RAII guard in cmd_run) are dropped before std::process::exit fires.
    // std::process::exit does NOT run destructors — the wrapper ensures cleanup.
    std::process::exit(run());
}

fn run() -> i32 {
    let cli = Cli::parse();
    match cli.command {
        Command::Check { source }         => cmd_check(source),
        Command::Build { source, output } => cmd_build(source, output),
        Command::Run   { source, args }   => cmd_run(source, args),
    }
}

// ─── Subcommand handlers ──────────────────────────────────────────────────────

fn cmd_check(source: PathBuf) -> i32 {
    run_pipeline(&source, |_ast, result| {
        // Deferred constructs are non-fatal for check — report as notes.
        for d in &result.deferred {
            eprintln!("ofan: note: {d}");
        }
        eprintln!("ofan: check ok");
        0
    })
}

fn cmd_build(source: PathBuf, output: Option<PathBuf>) -> i32 {
    // Resolve output path before borrowing source in run_pipeline.
    // Default: <stem>[.exe] in CWD — deliberately NOT next to the source file.
    // Prior behavior placed the binary next to the source; this was changed to
    // avoid cluttering .ofn directories with build artifacts.
    let out = match output {
        Some(p) => p,
        None => match source.file_stem() {
            Some(stem) => PathBuf::from(stem).with_extension(std::env::consts::EXE_EXTENSION),
            None => {
                eprintln!(
                    "ofan: cannot derive output name from '{}'; pass -o <output>",
                    source.display()
                );
                return 1;
            }
        },
    };
    run_pipeline(&source, move |ast, result| {
        if result.has_deferred() {
            for d in &result.deferred { eprintln!("ofan: unsupported: {d}"); }
            eprintln!("ofan: cannot compile: source contains unresolved constructs");
            return 1;
        }
        match emit_to(ast, &result, &out) {
            Ok(()) => 0,
            Err(code) => code,
        }
    })
}

fn cmd_run(source: PathBuf, args: Vec<String>) -> i32 {
    // Validate stem early; extract before closure so `source` can be borrowed by
    // run_pipeline while `stem`/`args` are moved in. PID suffix prevents collision
    // when two concurrent `ofan run` invocations target the same source file.
    let stem = match source.file_stem() {
        Some(s) => {
            let mut name = s.to_os_string();
            name.push(format!("_{}", std::process::id()));
            name
        }
        None => {
            eprintln!(
                "ofan: cannot derive temp name from '{}'; pass a valid .ofn path",
                source.display()
            );
            return 1;
        }
    };
    run_pipeline(&source, move |ast, result| {
        if result.has_deferred() {
            for d in &result.deferred { eprintln!("ofan: unsupported: {d}"); }
            eprintln!("ofan: cannot compile: source contains unresolved constructs");
            return 1;
        }
        let tmp = std::env::temp_dir()
            .join(&stem)
            .with_extension(std::env::consts::EXE_EXTENSION);
        // RAII guard: temp binary is removed when this closure returns, whether
        // normally or via an early return. Cleanup happens before run() returns
        // to main(), which then calls std::process::exit — so no leak occurs.
        let _guard = TempBinary(tmp.clone());
        if let Err(code) = emit_to(ast, &result, &tmp) {
            return code;
        }
        match std::process::Command::new(&tmp).args(&args).status() {
            Ok(status) => status.code().unwrap_or(1),
            Err(e) => {
                eprintln!(
                    "ofan: run error: {e} — check that '{}' is executable \
                     and the temp directory allows execution",
                    tmp.display()
                );
                1
            }
        }
    })
}

// ─── Shared pipeline ──────────────────────────────────────────────────────────

/// Run the lex → parse → typecheck pipeline, then call `f` with the AST and
/// inference result.  `Ast<'src>` borrows `&'src str` slices from the source
/// string, so the source must remain alive while `f` runs — the continuation
/// pattern keeps the source in scope on this stack frame.
fn run_pipeline<F>(source_path: &Path, f: F) -> i32
where
    F: FnOnce(&ast::Ast<'_>, typechecker::InferResult) -> i32,
{
    let src = match std::fs::read_to_string(source_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("ofan: cannot read '{}': {e}", source_path.display());
            return 1;
        }
    };
    let tokens = match lexer::Lexer::new(&src).lex() {
        Ok(t) => t,
        Err(e) => { eprintln!("ofan: lex error: {e}"); return 1; }
    };
    let ast = match parser::Parser::new(tokens).parse() {
        Ok(a) => a,
        Err(e) => { eprintln!("ofan: parse error: {e}"); return 1; }
    };
    match typechecker::infer(&ast) {
        Ok(result) => f(&ast, result),
        Err(errs) => {
            for e in &errs { eprintln!("ofan: type error: {e}"); }
            1
        }
    }
}

// ─── Codegen dispatch ─────────────────────────────────────────────────────────

#[cfg(feature = "codegen")]
fn emit_to(
    ast: &ast::Ast<'_>,
    result: &typechecker::InferResult,
    out: &Path,
) -> Result<(), i32> {
    use ofan::codegen::llvm::LlvmContext;
    let ctx = LlvmContext::new();
    ctx.emit(ast, result, out).map_err(|e| {
        eprintln!("ofan: codegen error: {e}");
        1_i32
    })?;
    eprintln!("ofan: compiled \u{2192} {}", out.display());
    Ok(())
}

#[cfg(not(feature = "codegen"))]
fn emit_to(
    _ast: &ast::Ast<'_>,
    _result: &typechecker::InferResult,
    _out: &Path,
) -> Result<(), i32> {
    eprintln!("ofan: `build` and `run` require codegen support");
    eprintln!("ofan: rebuild the compiler with: cargo build --features codegen");
    Err(1)
}

// ─── TempBinary RAII guard ────────────────────────────────────────────────────

struct TempBinary(PathBuf);

impl Drop for TempBinary {
    fn drop(&mut self) {
        // Ignore errors: the file may not exist if codegen failed before writing it.
        let _ = std::fs::remove_file(&self.0);
    }
}
