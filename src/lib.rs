pub mod ast;
pub mod env;
pub mod error;
pub mod lexer;
pub mod loader;
pub mod parser;
pub mod types;
// pub mod typecheck;
pub mod eval;

use miette::Result;
use std::path::Path;

use crate::{
    ast::{Expr, Program},
    env::Envs,
    error::TiflError,
    lexer::{Lexer, Token},
    parser::Parser,
};
use loader::load_program_with_includes;

pub fn run_from_files(prelude_path: &Path, expr_src: &str) -> miette::Result<String> {
    let program = load_program_with_includes(prelude_path)?;
    let expr = parse_expr_src(expr_src)?;
    let envs = Envs::from_program(&program).map_err(miette::Report::new)?;

    // Resolve nominal types into semantic forms
    let resolved = crate::types::resolve_nominals(&envs.types).map_err(miette::Report::new)?;

    // static typecheck before executing
    /* let expr_ty = crate::typecheck::type_of_expr(&expr, &envs.types, &envs.values, &resolved)
    .map_err(miette::Report::new)?; */

    let value = eval::eval_expr(&expr, &envs.values, &resolved).map_err(miette::Report::new)?;

    Ok(value.to_string())
}

pub fn run(program_src: &str, expr_src: &str) -> miette::Result<String> {
    let program = parse_program_src(program_src)?;
    let expr = parse_expr_src(expr_src)?;
    let envs = Envs::from_program(&program).map_err(miette::Report::new)?;

    let resolved = crate::types::resolve_nominals(&envs.types).map_err(miette::Report::new)?;

    /* let expr_ty = crate::typecheck::type_of_expr(&expr, &envs.types, &envs.values, &resolved)
    .map_err(miette::Report::new)?; */

    let value =
        crate::eval::eval_expr(&expr, &envs.values, &resolved).map_err(miette::Report::new)?;

    Ok(value.to_string())
}

fn lex(src: &str) -> miette::Result<Vec<Token>> {
    Lexer::new(src)
        .collect::<Result<Vec<Token>, TiflError>>()
        .map_err(|e| miette::Report::new(e))
}

pub fn parse_program_src(program_src: &str) -> Result<Program> {
    let tokens = lex(program_src)?;
    let mut p = Parser::new(tokens);
    p.parse_program().map_err(miette::Report::new)
}

pub fn parse_expr_src(expr_src: &str) -> miette::Result<Expr> {
    let tokens = lex(expr_src)?;
    let mut p = Parser::new(tokens);
    p.parse_expr_entry().map_err(miette::Report::new)
}

#[test]
fn parse_program_and_expr_smode() {
    let program_src = r#"id=\x:T.x, Pair={a:A,b:B}, Opt=[none,some:A]"#;
    let expr_src = r#"id x"#;

    let prog = crate::parse_program_src(program_src).unwrap();
    let expr = crate::parse_expr_src(expr_src).unwrap();

    // println!("{:#?}", prog);

    assert!(!prog.defs.is_empty());
    assert!(matches!(expr, crate::ast::Expr::Application { .. }))
}

