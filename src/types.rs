use std::collections::{HashMap, HashSet};
use crate::ast::{NominalType, TypeDef, TypeExpr, TypeRhs};
use crate::error::TypeLowerError;


#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Named(String),
    Arrow(Box<Type>, Box<Type>),
    Tuple(Vec<Type>),

    Struct(StructType),
    Union(UnionType),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructType {
    pub name: String,
    pub fields: HashMap<String, Type>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnionType {
    pub name: String,
    pub variants: HashMap<String, Option<Type>>,
}

pub fn lower_type_expr(expr: &TypeExpr) -> Type {
    match expr {
        TypeExpr::Named(name) => Type::Named(name.clone()),
        TypeExpr::Arrow(a, b) => 
            Type::Arrow(
                Box::new(lower_type_expr(a)), 
                Box::new(lower_type_expr(b)),

            ),
        TypeExpr::Tuple(items) => 
            Type::Tuple(items.iter().map(lower_type_expr).collect()),
    }
}

#[derive(Debug, Clone, Default)]
pub struct ResolvedTypeEnv {
    // Named: Type::Struct/Union
    pub named: HashMap<String, Type>,
    // Aliases that can be later lowered and resolved on demand
    pub alias: HashMap<String, TypeExpr>,
}

// Todo: Look at it again closer if nothing forgotten
fn typeexpr_mentions(
    target: &str,
    expr: &TypeExpr,
    type_defs: &HashMap<String, TypeDef>,
    seen: &mut HashSet<String>,
) -> bool {
    match expr {
        TypeExpr::Named(n) => {
            if n == target {
                return true;
            }

            // Avoid infinite loops in alias cycles / mutual recursion
            if !seen.insert(n.clone()) {
                return false;
            }

            match type_defs.get(n) {
                None => false, // unknown names handled elsewhere if needed
                Some(td) => match &td.rhs {
                    TypeRhs::Structural(te) => typeexpr_mentions(target, te, type_defs, seen),
                    TypeRhs::Nominal(NominalType::StructType(decls)) => decls
                        .iter()
                        .any(|d| typeexpr_mentions(target, &d.ty, type_defs, seen)),
                    TypeRhs::Nominal(NominalType::UnionType(elems)) => elems.iter().any(|e| {
                        e.elem_type
                            .as_ref()
                            .is_some_and(|t| typeexpr_mentions(target, t, type_defs, seen))
                    }),
                },
            }
        }

        TypeExpr::Arrow(a, b) => {
            typeexpr_mentions(target, a, type_defs, seen)
                || typeexpr_mentions(target, b, type_defs, seen)
        }

        TypeExpr::Tuple(items) => items
            .iter()
            .any(|t| typeexpr_mentions(target, t, type_defs, seen)),
    }
}

/// Build resolved nominal types (struct/union) from TypeDef env
/// Call this right before typechecking
pub fn resolve_nominals(type_defs: &HashMap<String, TypeDef>) -> Result<ResolvedTypeEnv, TypeLowerError> {
    let mut out = ResolvedTypeEnv::default();

    for (name, td) in type_defs {
        match &td.rhs {
            TypeRhs::Nominal(nom) => {
                let ty = match nom {
                    NominalType::StructType(decls) => {
                        let mut fields = HashMap::new();
                        for d in decls {
                            if fields.contains_key(&d.name) {
                                return Err(TypeLowerError::DuplicateStructField {
                                    ty: name.clone(),
                                    field: d.name.clone(),
                                });
                            }
                            fields.insert(d.name.clone(), lower_type_expr(&d.ty));
                        }
                        Type::Struct(StructType { name: name.clone(), fields })
                    }

                    NominalType::UnionType(elems) => {
                        // Unguarded recursive union check
                        let mut any_recursive = false;
                        let mut has_base_case = false;

                        for e in elems {
                            match &e.elem_type {
                                None => {
                                    // simple label => recursion-free alternative exists
                                    has_base_case = true;
                                    break;
                                }
                                Some(te) => {
                                    let mut seen = HashSet::new();
                                    let rec = typeexpr_mentions(name, te, type_defs, &mut seen);
                                    if rec {
                                        any_recursive = true;
                                    } else {
                                        has_base_case = true;
                                        break;
                                    }
                                }
                            }
                        }

                        if any_recursive && !has_base_case {
                            return Err(TypeLowerError::UnguardedRecursiveUnion { ty: name.clone() });
                        }


                        let mut variants = HashMap::new();
                        for e in elems {
                            if variants.contains_key(&e.name) {
                                return Err(TypeLowerError::DuplicateUnionVariant {
                                    ty: name.clone(),
                                    variant: e.name.clone(),
                                });
                            }
                            let payload = e.elem_type.as_ref().map(lower_type_expr);
                            variants.insert(e.name.clone(), payload);
                        }
                        Type::Union(UnionType { name: name.clone(), variants })
                    }
                };
                out.named.insert(name.clone(), ty);
            }
            TypeRhs::Structural(texp) => {
                out.alias.insert(name.clone(), texp.clone());

            }
        }
    }

    Ok(out)
}


/*
    Needed because in multiple places, we don't just want the name of a type but also its structure
    Union => Which labels exist
    Struct => Which fields exist
    But expressions and annotations mostly refer to names like U or Pair
    So this is the bridge
    Type::Named("U") -> Type::Union(...) or alias expansion (S=T)
 */
pub fn resolve_named<'a>(t: &'a Type, env: &'a ResolvedTypeEnv) -> Option<Type> {
    fn go(name: &str, env: &ResolvedTypeEnv, seen: &mut std::collections::HashSet<String>) -> Option<Type> {
        if !seen.insert(name.to_string()) { return None; } // cycle
        if let Some(nom) = env.named.get(name) {
            return Some(nom.clone());
        }
        if let Some(texp) = env.alias.get(name) {
            let lowered = lower_type_expr(texp);
            // if alias points to another name, keep resolving
            if let Type::Named(n2) = &lowered {
                return go(n2, env, seen);
            }
            return Some(lowered);
        }
        None
    }

    match t {
        Type::Named(n) => go(n, env, &mut HashSet::new()),
        _ => Some(t.clone()),
    }
}



// Regression tests added after Union recursivity check:
#[test]
fn unguarded_recursive_union_is_rejected() {
    use crate::ast::*;

    let mut defs = HashMap::new();
    defs.insert(
        "Bad".into(),
        TypeDef {
            name: "Bad".into(),
            rhs: TypeRhs::Nominal(NominalType::UnionType(vec![
                Elem { name: "a".into(), elem_type: Some(TypeExpr::Named("Bad".into())) }
            ])),
        },
    );

    let err = resolve_nominals(&defs).unwrap_err();
    assert!(format!("{err}").contains("unguarded recursive union"));
}

#[test]
fn guarded_recursive_union_is_ok() {
    use crate::ast::*;

    let mut defs = HashMap::new();
    defs.insert(
        "List".into(),
        TypeDef {
            name: "List".into(),
            rhs: TypeRhs::Nominal(NominalType::UnionType(vec![
                Elem { name: "nil".into(), elem_type: None },
                Elem {
                    name: "cons".into(),
                    elem_type: Some(TypeExpr::Tuple(vec![
                        TypeExpr::Named("Bool".into()),
                        TypeExpr::Named("List".into()),
                    ])),
                },
            ])),
        },
    );
    defs.insert(
        "Bool".into(),
        TypeDef {
            name: "Bool".into(),
            rhs: TypeRhs::Nominal(NominalType::UnionType(vec![
                Elem { name: "true".into(), elem_type: None },
                Elem { name: "false".into(), elem_type: None },
            ])),
        },
    );

    resolve_nominals(&defs).unwrap();
}