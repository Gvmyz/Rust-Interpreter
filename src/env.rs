use std::{collections::HashMap};

use crate::ast::{Def, Expr, Program, TypeDef};
use crate::error::EnvBuildError;

// Program-scope environment
#[derive(Clone, Debug, Default)]
pub struct Envs {
    // Global Types     (Bool, Int, ...)
    pub types: TypeEnv,
    // Global Values    (not, and, ...)
    pub values: ValueEnv,
}

pub type TypeEnv = HashMap<String, TypeDef>;
pub type ValueEnv = HashMap<String, Expr>;

impl Envs {
    /// Build environments from a fully-expanded program.
    /// Program-scope names must be unique (no global shadowing).
    pub fn from_program(program: &Program) -> Result<Self, EnvBuildError> {
        let mut envs = Envs::default();

        for def in &program.defs {
            match def {
                Def::Type(td) => {
                    if envs.types.insert(td.name.clone(), td.clone()).is_some() {
                        return Err(EnvBuildError::DuplicateType {
                            name: td.name.clone(),
                        });
                    }
                }
                Def::Value(vd) => {
                    if envs.values.insert(vd.name.clone(), (*vd.expr).clone()).is_some() {
                        return Err(EnvBuildError::DuplicateValue {
                            name: vd.name.clone(),
                        });
                    }
                }
                Def::Include(_) => {
                    // Should never happen after expansion; ignore safely.
                }
            }
        }
        
        Ok(envs)
    }
}



#[test]
fn env_rejects_duplicate_value_defs() {
    use crate::ast::*;

    let program = Program {
        defs: vec![
            Def::Value(ValueDef {
                name: "x".into(),
                expr: Box::new(Expr::Atom(Atom::Var("old".into()))),
            }),
            Def::Value(ValueDef {
                name: "x".into(),
                expr: Box::new(Expr::Atom(Atom::Var("new".into()))),
            }),
        ],
    };

    let err = crate::env::Envs::from_program(&program).unwrap_err();
    let msg = format!("{err:?}");
    println!("{msg}");
    assert!(msg.contains("DuplicateValue"));
}