use miette::Diagnostic;
use std::collections::{HashMap, HashSet};

use crate::{
    ast::{Atom, Expr, Spec, ValueDef},
    env::{TypeEnv, ValueEnv},
    types::{lower_type_expr, resolve_named, ResolvedTypeEnv, Type, StructType, UnionType},
};

pub type Gamma = HashMap<String, Type>;

#[derive(Debug, thiserror::Error, Diagnostic)]
pub enum TypeError {
    #[error("unbound variable '{0}'")]
    UnboundVar(String),

    #[error("expected function type, got {0:?}")]
    NotAFunction(Type),

    #[error("arg type mismatch: expected {expected:?}, got {got:?}")]
    ArgMismatch { expected: Type, got: Type },

    #[error("unknown type '{0}'")]
    UnknownType(String),

    #[error("not a struct/tuple: cannot access .{field} on {base:?}")]
    BadAccess { base: Type, field: String },

    #[error("unknown struct field '{field}' on '{ty}'")]
    UnknownStructField { ty: String, field: String },

    #[error("tuple index must be >=1, got .{0}")]
    BadTupleIndex(usize),

    #[error("tuple index out of bounds: .{index} on tuple of len {len}")]
    TupleIndexOob { index: usize, len: usize },

    #[error("'{ty}' is not a union type")]
    NotAUnion { ty: String },

    #[error("unknown union label '{label}' for union '{ty}'")]
    UnknownUnionLabel { ty: String, label: String },

    #[error("union label '{label}' of '{ty}' expects payload of type {expected:?}")]
    MissingPayload { ty: String, label: String, expected: Type },

    #[error("union label '{label}' of '{ty}' expects no payload")]
    UnexpectedPayload { ty: String, label: String },

    #[error("union payload mismatch for '{ty}[{label}=...]': expected {expected:?}, got {got:?}")]
    UnionPayloadMismatch { ty: String, label: String, expected: Type, got: Type },

    #[error("struct init missing field '{field}' for '{ty}'")]
    MissingStructField { ty: String, field: String },

    #[error("struct init unknown field '{field}' for '{ty}'")]
    UnknownStructInitField { ty: String, field: String },

    #[error("struct field '{field}' type mismatch: expected {expected:?}, got {got:?}")]
    StructFieldMismatch { ty: String, field: String, expected: Type, got: Type },

    #[error("case on non-union type {0:?}")]
    CaseOnNonUnion(Type),

    #[error("case branch '{label}' not in union '{ty}'")]
    CaseUnknownLabel { ty: String, label: String },

    #[error("case branch for label '{label}' must be a function of type {expected_arg:?} -> R")]
    CaseBranchMustBeFunction { label: String, expected_arg: Type },

    #[error("case branches disagree on result type: {0:?} vs {1:?}")]
    CaseResultMismatch(Type, Type),

    #[error("non-exhaustive case on '{ty}', missing: {missing:?}")]
    NonExhaustiveCase { ty: String, missing: Vec<String> },

    #[error("default clause type not compatible with remaining labels")]
    BadDefaultClause,
}

pub fn type_of_expr(
    expr: &Expr,
    tenv_raw: &TypeEnv,
    venv: &ValueEnv,
    resolved: &ResolvedTypeEnv,
) -> Result<Type, TypeError> {
    let mut gamma = Gamma::new();
    let mut visiting = HashSet::new();
    type_of(expr, tenv_raw, venv, resolved, &mut gamma, &mut visiting)
}

fn type_of(
    expr: &Expr,
    tenv_raw: &TypeEnv,
    venv: &ValueEnv,
    resolved: &ResolvedTypeEnv,
    gamma: &mut Gamma,
    visiting: &mut HashSet<String>,
) -> Result<Type, TypeError> {
    match expr {
        Expr::Lambda { param, body } => {
            let a = lower_type_expr(&param.ty);
            gamma.insert(param.name.clone(), a.clone());
            let b = type_of(body, tenv_raw, venv, resolved, gamma, visiting)?;
            gamma.remove(&param.name);
            Ok(Type::Arrow(Box::new(a), Box::new(b)))
        }

        Expr::Application { fun, arg } => {
            let tf = type_of(fun, tenv_raw, venv, resolved, gamma, visiting)?;
            let ta = type_of(arg, tenv_raw, venv, resolved, gamma, visiting)?;

            match tf {
                Type::Arrow(a, b) => {
                    if *a == ta { Ok(*b) } else {
                        Err(TypeError::ArgMismatch { expected: *a, got: ta })
                    }
                }
                other => Err(TypeError::NotAFunction(other)),
            }
        }

        Expr::Atom(a) => type_of_atom(a, tenv_raw, venv, resolved, gamma, visiting),
    }
}


fn type_of_atom(
    atom: &Atom,
    tenv_raw: &TypeEnv,
    venv: &ValueEnv,
    resolved: &ResolvedTypeEnv,
    gamma: &mut Gamma,
    visiting: &mut HashSet<String>,
) -> Result<Type, TypeError> {
    match atom {
        Atom::Var(x) => {
            if let Some(t) = gamma.get(x) {
                return Ok(t.clone());
            }
            if let Some(e) = venv.get(x) {
                if !visiting.insert(x.clone()) {
                    return Err(TypeError::UnboundVar(x.clone())); // or RecursiveDef
                }
                let t = type_of(e, tenv_raw, venv, resolved, gamma, visiting)?;
                visiting.remove(x);
                return Ok(t);
            }
            Err(TypeError::UnboundVar(x.clone()))
        }

        Atom::Tuple(items) => {
            let tys = items.iter()
                .map(|e| type_of(e, tenv_raw, venv, resolved, gamma, visiting))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Type::Tuple(tys))
        }

        Atom::Paren(e) => type_of(e, tenv_raw, venv, resolved, gamma, visiting),

        Atom::Access { base, field } => {
            let base_ty = type_of_atom(base, tenv_raw, venv, resolved, gamma, visiting)?;
            type_of_access(base_ty, field, resolved)
        }

        Atom::Typed { ty_name, spec } => type_of_typed(ty_name, spec, tenv_raw, venv, resolved, gamma, visiting),

        Atom::Case { scrutinee, branches, default } => {
            let scrut_ty = type_of_atom(scrutinee, tenv_raw, venv, resolved, gamma, visiting)?;
            type_of_case(scrut_ty, branches, default.as_deref(), tenv_raw, venv, resolved, gamma, visiting)
        }
    }
}


fn type_of_access(base_ty: Type, field: &str, resolved: &ResolvedTypeEnv) -> Result<Type, TypeError> {
    // Tuple indexing is 1-based
    if let Ok(n) = field.parse::<usize>() {
        if n == 0 {
            return Err(TypeError::BadTupleIndex(n));
        }
        if let Type::Tuple(items) = base_ty {
            let idx = n - 1;
            if idx < items.len() {
                return Ok(items[idx].clone());
            } else {
                return Err(TypeError::TupleIndexOob { index: n, len: items.len() });
            }
        }
    }

    // Resolve aliases + nominal types
    let resolved_base = resolve_named(&base_ty, resolved).unwrap_or(base_ty.clone());

    match resolved_base {
        Type::Struct(StructType { name, fields }) => {
            fields.get(field).cloned().ok_or_else(|| TypeError::UnknownStructField {
                ty: name,
                field: field.to_string(),
            })
        }
        other => Err(TypeError::BadAccess { base: other, field: field.to_string() }),
    }
}

fn type_of_typed(
    ty_name: &str,
    spec: &Spec,
    tenv_raw: &TypeEnv,
    venv: &ValueEnv,
    resolved: &ResolvedTypeEnv,
    gamma: &mut Gamma,
    visiting: &mut HashSet<String>,
) -> Result<Type, TypeError> {
    let ty = resolved.named.get(ty_name).ok_or_else(|| TypeError::UnknownType(ty_name.to_string()))?;

    match ty {
        Type::Union(u) => type_of_union_ctor(u, spec, tenv_raw, venv, resolved, gamma, visiting),
        Type::Struct(s) => type_of_struct_ctor(s, spec, tenv_raw, venv, resolved, gamma, visiting),
        _ => Err(TypeError::UnknownType(ty_name.to_string())),
    }
}

fn type_of_union_ctor(
    u: &UnionType,
    spec: &Spec,
    tenv_raw: &TypeEnv,
    venv: &ValueEnv,
    resolved: &ResolvedTypeEnv,
    gamma: &mut Gamma,
    visiting: &mut HashSet<String>,
) -> Result<Type, TypeError> {
    match spec {
        // U[z] must be a simple label with no payload
        Spec::UnionLabel(lbl) => match u.variants.get(lbl) {
            Some(None) => Ok(Type::Named(u.name.clone())),
            // Expecting payload => .else_Err
            Some(Some(payload_ty)) => Err(TypeError::MissingPayload {
                ty: u.name.clone(),
                label: lbl.clone(),
                expected: payload_ty.clone(),
            }),
            None => Err(TypeError::UnknownUnionLabel { ty: u.name.clone(), label: lbl.clone() }),
        },

        // U[x=a] must be a field label with payload type A and a:A
        Spec::UnionField(vdef) => {
            let lbl = &vdef.name;
            match u.variants.get(lbl) {
                Some(Some(payload_ty)) => {
                    let got = type_of(&vdef.expr, tenv_raw, venv, resolved, gamma, visiting)?;
                    if &got == payload_ty {
                        Ok(Type::Named(u.name.clone()))
                    } else {
                        Err(TypeError::UnionPayloadMismatch {
                            ty: u.name.clone(),
                            label: lbl.clone(),
                            expected: payload_ty.clone(),
                            got,
                        })
                    }
                }
                Some(None) => Err(TypeError::UnexpectedPayload { ty: u.name.clone(), label: lbl.clone() }),
                None => Err(TypeError::UnknownUnionLabel { ty: u.name.clone(), label: lbl.clone() }),
            }
        }

        Spec::StructFields(_) => Err(TypeError::NotAUnion { ty: u.name.clone() }),
    }
}

fn type_of_struct_ctor(
    s: &StructType,
    spec: &Spec,
    tenv_raw: &TypeEnv,
    venv: &ValueEnv,
    resolved: &ResolvedTypeEnv,
    gamma: &mut Gamma,
    visiting: &mut HashSet<String>,
) -> Result<Type, TypeError> {
    let fields_given = match spec {
        Spec::StructFields(fs) => fs,
        _ => return Err(TypeError::UnknownType(s.name.clone())),
    };

    // Check given fields + types
    let mut seen = HashSet::<String>::new();
    for f in fields_given {
        let expected = s.fields.get(&f.name).ok_or_else(|| TypeError::UnknownStructInitField {
            ty: s.name.clone(),
            field: f.name.clone(),
        })?;
        let got = type_of(&f.expr, tenv_raw, venv, resolved, gamma, visiting)?;
        if &got != expected {
            return Err(TypeError::StructFieldMismatch {
                ty: s.name.clone(),
                field: f.name.clone(),
                expected: expected.clone(),
                got,
            });
        }
        seen.insert(f.name.clone());
    }

    // Spec examples imply you should provide all fields; otherwise you can’t safely use them.
    for req in s.fields.keys() {
        if !seen.contains(req) {
            return Err(TypeError::MissingStructField {
                ty: s.name.clone(),
                field: req.clone(),
            });
        }
    }

    Ok(Type::Named(s.name.clone()))
}


fn type_of_case(
    scrut_ty: Type,
    branches: &[ValueDef],
    default: Option<&Expr>,
    tenv_raw: &TypeEnv,
    venv: &ValueEnv,
    resolved: &ResolvedTypeEnv,
    gamma: &mut Gamma,
    visiting: &mut HashSet<String>,
) -> Result<Type, TypeError> {
    // Scrutinee must be a union. :contentReference[oaicite:9]{index=9}
    let scrut_res = resolve_named(&scrut_ty, resolved).unwrap_or(scrut_ty.clone());
    let u = match scrut_res {
        Type::Union(u) => u,
        _ => return Err(TypeError::CaseOnNonUnion(scrut_ty)),
    };

    // Determine common result type R by checking branches according to label kind.
    // - field label (payload): branch must be A -> R
    // - simple label: branch must be R
    let mut covered = HashSet::<String>::new();
    let mut result_ty: Option<Type> = None;

    for b in branches {
        let lbl = &b.name;
        let payload_opt = u.variants.get(lbl).ok_or_else(|| TypeError::CaseUnknownLabel {
            ty: u.name.clone(),
            label: lbl.clone(),
        })?;

        let bt = type_of(&b.expr, tenv_raw, venv, resolved, gamma, visiting)?;

        let r = match payload_opt {
            // simple label => value of type R
            None => bt,

            // field label => must be function A -> R
            Some(arg_ty) => match bt {
                Type::Arrow(a, r) => {
                    if *a == arg_ty.clone() {
                        *r
                    } else {
                        return Err(TypeError::ArgMismatch { expected: arg_ty.clone(), got: *a });
                    }
                }
                _ => {
                    return Err(TypeError::CaseBranchMustBeFunction {
                        label: lbl.clone(),
                        expected_arg: arg_ty.clone(),
                    });
                }
            },
        };

        if let Some(r0) = &result_ty {
            if *r0 != r {
                return Err(TypeError::CaseResultMismatch(r0.clone(), r));
            }
        } else {
            result_ty = Some(r);
        }

        covered.insert(lbl.clone());
    }

    // !! Possibly check here with some examples before presentation
    // Default clause: must be compatible with all remaining labels
    if let Some(d) = default {
        let dt = type_of(d, tenv_raw, venv, resolved, gamma, visiting)?;
        // We can only accept default if all remaining labels require the SAME branch “shape”
        // and yield the same result type.
        //
        // In the spec, default is a <vexp>. That means:
        // - if remaining labels include any payload labels, default must be a function A->R,
        //   but ONLY if all those payload labels have the same A (rare).
        // - otherwise, default can be a value R.
        //
        // So we enforce: remaining payload argument types are either none, or all identical.
        let mut remaining_payload_arg: Option<Type> = None;
        for (lbl, payload) in &u.variants {
            if covered.contains(lbl) { continue; }
            if let Some(arg) = payload {
                match &remaining_payload_arg {
                    None => remaining_payload_arg = Some(arg.clone()),
                    Some(prev) if prev == arg => {}
                    Some(_) => return Err(TypeError::BadDefaultClause),
                }
            }
        }

        // In case of payload, check if r in a->r is correct with dt versus remaining_payload_arg
        let r_from_default = match remaining_payload_arg {
            None => dt,
            Some(arg) => match dt {
                Type::Arrow(a, r) if *a == arg => *r,
                _ => return Err(TypeError::BadDefaultClause),
            },
        };

        if let Some(r0) = &result_ty {
            if *r0 != r_from_default {
                return Err(TypeError::CaseResultMismatch(r0.clone(), r_from_default));
            }
            return Ok(r0.clone());
        } else {
            return Ok(r_from_default);
        }
    }

    // No default => must be exhaustive (COVER ALL LABELS)
    let mut missing = Vec::new();
    for lbl in u.variants.keys() {
        if !covered.contains(lbl) {
            missing.push(lbl.clone());
        }
    }
    if !missing.is_empty() {
        // Sorting for Err
        missing.sort();
        return Err(TypeError::NonExhaustiveCase { ty: u.name.clone(), missing });
    }

    Ok(result_ty.expect("case must have at least one branch or default"))
}