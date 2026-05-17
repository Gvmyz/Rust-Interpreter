use miette::{Result};
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "TIFL Interpreter")]
#[command(version)]
#[command(about)]
struct Args {
    /// TIFL Prelude file to read
    prelude: std::path::PathBuf,

    /// Expression to run
    expression: String,
}

fn main() -> Result<()> {
    let args = Args::parse();

    /* let program_src = std::fs::read_to_string(&args.prelude).into_diagnostic()?;

    let out = tifl_interpreter::run(&program_src, &args.expression)?;
 */
    let out = tifl_interpreter::run_from_files(&args.prelude, &args.expression)?;

    println!("{out}");
    Ok(())
}




#[test]
fn verify_cli() {
    use clap::CommandFactory;
    Args::command().debug_assert();
}
