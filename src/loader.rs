use std::{collections::HashMap, path::{Path, PathBuf}};

use crate::{ast::{Def, Program}, parse_program_src};

use miette::{IntoDiagnostic, Result};


#[derive(Debug, thiserror::Error)]
pub enum IncludeError {
    #[error("include cycle detected: {0}")]
    Cycle(String),

    #[error("include path has no parent directory: {0}")]
    NoParent(String),
}

pub fn load_program_with_includes(entry_file: &Path) -> Result<Program> {
    let mut cache: HashMap<PathBuf, Program> = HashMap::new();
    let mut stack: Vec<PathBuf> = Vec::new();

    load_recursive(entry_file, &mut cache, &mut stack)
}

fn load_recursive(
    file: &Path,
    cache: &mut HashMap<PathBuf, Program>, 
    stack: &mut Vec<PathBuf>,
) -> Result<Program> {
    let canon = std::fs::canonicalize(file).into_diagnostic()?;

    // Cache hit
    if let Some(p) = cache.get(&canon) {
        return Ok(p.clone());
    }

    if let Some(idx) = stack.iter().position(|p| p == &canon) {
        let cycle = stack[idx..]
            .iter()
            .chain(std::iter::once(&canon))
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(" -> ");
        return Err(IncludeError::Cycle(cycle)).into_diagnostic()?;
    }

    stack.push(canon.clone());

    let src = std::fs::read_to_string(&canon).into_diagnostic()?;
    let parsed = parse_program_src(&src)?;

    let base_dir = canon
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| IncludeError::NoParent(canon.display().to_string()))
        .into_diagnostic()?;

    // Expand includes in place
    let mut out_defs: Vec<Def> = Vec::new();
    for def in parsed.defs {
        match def {
            Def::Include(inc) => {
                let inc_path = base_dir.join(inc.file_name);
                let inc_prog = load_recursive(&inc_path, cache, stack)?;
                out_defs.extend(inc_prog.defs);
            },
            other => out_defs.push(other),
        }
    }

    let out = Program { defs: out_defs };

    // Done with this node
    stack.pop();

    cache.insert(canon, out.clone());
    Ok(out)
}

#[test]
fn include_expands_in_place() {
    use std::fs;

    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.tifl");
    let b = dir.path().join("b.tifl");

    fs::write(&b, r#"id=\x:T.x"#).unwrap();
    fs::write(&a, r#"@b.tifl,k=\x:T.\y:U.x"#).unwrap();

    let prog = crate::loader::load_program_with_includes(&a).unwrap();
    assert!(prog.defs.iter().all(|d| !matches!(d, crate::ast::Def::Include(_))));
    assert_eq!(prog.defs.len(), 2);
}

#[test]
fn include_cycle_detected() {
    use std::fs;

    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.tifl");
    let b = dir.path().join("b.tifl");

    fs::write(&a, r#"@b.tifl"#).unwrap();
    fs::write(&b, r#"@a.tifl"#).unwrap();

    let err = crate::loader::load_program_with_includes(&a).unwrap_err();
    let msg = format!("{err:?}");
    assert!(msg.contains("include cycle"));
}
