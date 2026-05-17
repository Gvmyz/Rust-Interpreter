use crate::{
    ast::{Atom, Expr, Spec, ValueDef},
    env::ValueEnv,
    error::EvalError,
    types::{ResolvedTypeEnv, StructType, Type, UnionType, lower_type_expr, resolve_named},
};
use core::fmt;
use std::collections::{HashMap, HashSet};

// Produced Values (runtime data model) (also used for case checks, etc)
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Tuple(Vec<Value>),
    Struct {
        ty: String,
        fields: HashMap<String, Value>,
    },
    Union {
        ty: String,
        label: String,
        payload: Option<Box<Value>>,
    },
    // Lambda captures variables => need env
    Closure {
        param: String,
        param_ty: Type,
        ret_ty: Option<Type>,
        body: Box<Expr>,
        env: Env,
    },
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Tuple(xs) => {
                write!(f, "(")?;
                for (i, x) in xs.iter().enumerate() {
                    if i > 0 {
                        write!(f, ",")?;
                    }
                    write!(f, "{x}")?;
                }
                write!(f, ")")
            }

            Value::Struct { ty, fields } => {
                let mut keys: Vec<_> = fields.keys().collect();
                keys.sort();
                write!(f, "{ty}{{")?;
                for (i, k) in keys.iter().enumerate() {
                    if i > 0 {
                        write!(f, ",")?;
                    }
                    write!(f, "{}={}", k, fields.get(*k).unwrap())?;
                }
                write!(f, "}}")
            }

            Value::Union { ty, label, payload } => match payload {
                None => write!(f, "{ty}[{label}]"),
                Some(v) => write!(f, "{ty}[{label}={v}]"),
            },

            Value::Closure { .. } => {
                // printing full closures is messy => Printing placeholder
                write!(f, "<fun>")
            }
        }
    }
}

// Runtime local variable environment (lambda parameters and captured variables)
pub type Env = HashMap<String, Value>;

// Global runtime state
#[derive(Default)]
pub struct Globals {
    // => No reevaluation of globals
    cache: HashMap<String, Value>,
    // Used for cycle detection (global evaluation)
    visiting: std::collections::HashSet<String>,
    // Keyed by the address of the Atom::Case node
    // Try to ensure all branches return same result type
    case_result_ty: HashMap<usize, Type>,
}

fn type_of_value(v: &Value) -> Type {
    match v {
        Value::Tuple(xs) => Type::Tuple(xs.iter().map(type_of_value).collect()),
        Value::Struct { ty, .. } => Type::Named(ty.clone()),
        Value::Union { ty, .. } => Type::Named(ty.clone()),
        Value::Closure { param_ty, .. } => {
            // Return type unknown at runtime unless store it
            // For runtime checks, mainly need param type for application
            Type::Arrow(
                Box::new(param_ty.clone()),
                Box::new(Type::Named("<unknown>".into())),
            )
        }
    }
}

// Runtime type checker
fn ensure_type(val: &Value, expected: &Type, resolved: &ResolvedTypeEnv) -> Result<(), EvalError> {
    // Compare using resolved named types for alias handling
    // Compare "shapes" for tuples and nominal identity for struct/union.
    let got = type_of_value(val);

    // If expected is a name, allow alias resolution
    let exp_res = resolve_named(expected, resolved).unwrap_or(expected.clone());
    let got_res = resolve_named(&got, resolved).unwrap_or(got);

    // For nominal equality, compare canonical resolved types (names after resolution)
    if exp_res == got_res {
        Ok(())
    } else {
        Err(EvalError::RuntimeTypeMismatch {
            expected: expected.clone(),
            got: got_res,
        })
    }
}

pub fn eval_expr(
    expr: &Expr,
    venv: &ValueEnv,
    resolved: &ResolvedTypeEnv,
) -> Result<Value, EvalError> {
    let mut globals = Globals::default();
    let mut env = Env::new();
    eval(expr, &mut env, venv, resolved, &mut globals)
}

fn eval(
    expr: &Expr,
    env: &mut Env,
    venv: &ValueEnv,
    resolved: &ResolvedTypeEnv,
    globals: &mut Globals,
) -> Result<Value, EvalError> {
    match expr {
        Expr::Lambda { param, body } => {
            let param_ty = lower_type_expr(&param.ty);
            Ok(Value::Closure {
                param: param.name.clone(),
                param_ty,
                ret_ty: None,
                body: body.clone(),
                env: env.clone(), // capture
            })
        }

        Expr::Application { fun, arg } => {
            let f = eval(fun, env, venv, resolved, globals)?;
            let a = eval(arg, env, venv, resolved, globals)?;

            // Ensure the function value is a closure
            let (param, param_ty, ret_ty, body, mut clos_env) = match f {
                Value::Closure {
                    param,
                    param_ty,
                    ret_ty,
                    body,
                    env: clos_env,
                } => (param, param_ty, ret_ty, body, clos_env),
                _ => return Err(EvalError::NotAFunction),
            };

            ensure_type(&a, &param_ty, resolved)?;
            clos_env.insert(param, a);

            let res = eval(&body, &mut clos_env, venv, resolved, globals)?;

            // if closure carries a return-type constraint, enforce it
            if let Some(rt) = ret_ty {
                ensure_type(&res, &rt, resolved)?;
            }

            Ok(res)
        }

        Expr::Atom(a) => eval_atom(a, env, venv, resolved, globals),
    }
}

fn eval_atom(
    atom: &Atom,
    env: &mut Env,
    venv: &ValueEnv,
    resolved: &ResolvedTypeEnv,
    globals: &mut Globals,
) -> Result<Value, EvalError> {
    match atom {
        Atom::Var(x) => {
            if let Some(v) = env.get(x) {
                return Ok(v.clone());
            }
            eval_global(x, venv, resolved, globals)
        }

        Atom::Tuple(items) => {
            let mut out = Vec::new();
            for e in items {
                out.push(eval(e, env, venv, resolved, globals)?);
            }
            Ok(Value::Tuple(out))
        }

        Atom::Paren(e) => eval(e, env, venv, resolved, globals),

        Atom::Access { base, field } => {
            let b = eval_atom(base, env, venv, resolved, globals)?;
            eval_access(b, field)
        }

        Atom::Typed { ty_name, spec } => eval_typed(ty_name, spec, env, venv, resolved, globals),

        Atom::Case {
            scrutinee,
            branches,
            default,
        } => {
            let s = eval_atom(scrutinee, env, venv, resolved, globals)?;
            // Case_id = address of the AST node => stable ID for this
            let case_id = atom as *const Atom as usize;
            eval_case(
                case_id,
                s,
                branches,
                default.as_deref(),
                env,
                venv,
                resolved,
                globals,
            )
        }
    }
}

fn eval_global(
    name: &str,
    venv: &ValueEnv,
    resolved: &ResolvedTypeEnv,
    globals: &mut Globals,
) -> Result<Value, EvalError> {
    if let Some(v) = globals.cache.get(name) {
        return Ok(v.clone());
    }
    if !globals.visiting.insert(name.to_string()) {
        // recursion loop -> will diverge; report as unbound/recursive
        return Err(EvalError::UnboundVar(name.to_string()));
    }

    let expr = venv
        .get(name)
        .ok_or_else(|| EvalError::UnboundVar(name.to_string()))?;
    let mut empty_env = Env::new();
    let v = eval(expr, &mut empty_env, venv, resolved, globals)?;

    // Remove from visiting as it is checking cycles and no need anymore after that
    globals.visiting.remove(name);
    globals.cache.insert(name.to_string(), v.clone());
    Ok(v)
}

fn eval_access(base: Value, field: &str) -> Result<Value, EvalError> {
    if let Ok(n) = field.parse::<usize>() {
        if let Value::Tuple(items) = base {
            let idx = n.checked_sub(1).unwrap_or(usize::MAX);
            if idx < items.len() {
                return Ok(items[idx].clone());
            } else {
                return Err(EvalError::TupleIndexOob {
                    index: n,
                    len: items.len(),
                });
            }
        }
    }

    match base {
        Value::Struct { fields, .. } => {
            fields
                .get(field)
                .cloned()
                .ok_or_else(|| EvalError::UnknownField {
                    field: field.to_string(),
                })
        }
        _ => Err(EvalError::UnknownField {
            field: field.to_string(),
        }),
    }
}

fn eval_typed(
    ty_name: &str,
    spec: &Spec,
    env: &mut Env,
    venv: &ValueEnv,
    resolved: &ResolvedTypeEnv,
    globals: &mut Globals,
) -> Result<Value, EvalError> {
    // resolve type name (handle aliases)
    let ty = resolve_named(&Type::Named(ty_name.to_string()), resolved)
        .ok_or_else(|| EvalError::UnknownType(ty_name.to_string()))?;

    match ty {
        Type::Struct(s) => eval_struct_ctor(&s, spec, env, venv, resolved, globals),
        Type::Union(u) => eval_union_ctor(&u, spec, env, venv, resolved, globals),
        _ => Err(EvalError::NotNominalType(ty_name.to_string())),
    }
}

fn eval_struct_ctor(
    s: &StructType,
    spec: &Spec,
    env: &mut Env,
    venv: &ValueEnv,
    resolved: &ResolvedTypeEnv,
    globals: &mut Globals,
) -> Result<Value, EvalError> {
    let Spec::StructFields(fields_given) = spec else {
        return Err(EvalError::NotNominalType(s.name.clone()));
    };

    let mut out_fields: HashMap<String, Value> = HashMap::new();

    for f in fields_given {
        // Duplicates should error
        if out_fields.contains_key(&f.name) {
            return Err(EvalError::DuplicateStructField {
                ty: s.name.clone(),
                field: f.name.clone(),
            });
        }

        // Can only initialize declared fields
        let expected = s
            .fields
            .get(&f.name)
            .ok_or_else(|| EvalError::UnknownStructInitField {
                ty: s.name.clone(),
                field: f.name.clone(),
            })?;

        // compute result
        let v = eval(&f.expr, env, venv, resolved, globals)?;
        // Return value type checking
        let v = coerce_value_to_type(v, expected, resolved)?; // handles Arrow by attaching ret_ty

        out_fields.insert(f.name.clone(), v);
    }

    // enforce "all required fields present" (no optional fields)
    for req in s.fields.keys() {
        if !out_fields.contains_key(req) {
            return Err(EvalError::MissingStructField {
                ty: s.name.clone(),
                field: req.clone(),
            });
        }
    }

    Ok(Value::Struct {
        ty: s.name.clone(),
        fields: out_fields,
    })
}

fn eval_union_ctor(
    u: &UnionType,
    spec: &Spec,
    env: &mut Env,
    venv: &ValueEnv,
    resolved: &ResolvedTypeEnv,
    globals: &mut Globals,
) -> Result<Value, EvalError> {
    match spec {
        // ex: Bool[true]
        Spec::UnionLabel(lbl) => {
            let payload_ty = u
                .variants
                .get(lbl)
                .ok_or_else(|| EvalError::UnknownUnionLabel {
                    ty: u.name.clone(),
                    label: lbl.clone(),
                })?;

            // Err if payload
            match payload_ty {
                None => Ok(Value::Union {
                    ty: u.name.clone(),
                    label: lbl.clone(),
                    payload: None,
                }),
                Some(_) => Err(EvalError::MissingPayload { label: lbl.clone() }),
            }
        }

        // ex: OptBool[some=Bool[false]] => vs = payload expr
        Spec::UnionField(vd) => {
            let lbl = &vd.name;
            let payload_ty = u
                .variants
                .get(lbl)
                .ok_or_else(|| EvalError::UnknownUnionLabel {
                    ty: u.name.clone(),
                    label: lbl.clone(),
                })?;

            // Error if no payload
            match payload_ty {
                None => Err(EvalError::UnexpectedPayload { label: lbl.clone() }),
                Some(t) => {
                    // Must evaluate and match the return type
                    let v = eval(&vd.expr, env, venv, resolved, globals)?;
                    let v = coerce_value_to_type(v, t, resolved)?;
                    Ok(Value::Union {
                        ty: u.name.clone(),
                        label: lbl.clone(),
                        payload: Some(Box::new(v)),
                    })
                }
            }
        }

        Spec::StructFields(_) => Err(EvalError::NotNominalType(u.name.clone())),
    }
}

fn eval_case(
    case_id: usize,
    scrutinee: Value,
    branches: &[ValueDef],
    default: Option<&Expr>,
    env: &mut Env,
    venv: &ValueEnv,
    resolved: &ResolvedTypeEnv,
    globals: &mut Globals,
) -> Result<Value, EvalError> {
    // Scrutinee must be a Union while also destructuring important params
    let Value::Union { ty, label, payload } = scrutinee else {
        return Err(EvalError::CaseOnNonUnion);
    };

    // Resolve union type definition from scrutinee's nominal type name
    let ty_resolved = resolve_named(&Type::Named(ty.clone()), resolved)
        .ok_or_else(|| EvalError::UnknownType(ty.clone()))?;

    let u = match ty_resolved {
        Type::Union(u) => u,
        _ => return Err(EvalError::NotNominalType(ty)),
    };

    // Validate branches: no duplicates + labels exist
    let mut covered = HashSet::<String>::new();
    for b in branches {
        if !covered.insert(b.name.clone()) {
            return Err(EvalError::DuplicateCaseBranch(b.name.clone()));
        }
        if !u.variants.contains_key(&b.name) {
            return Err(EvalError::CaseUnknownLabel {
                ty: u.name.clone(),
                label: b.name.clone(),
            });
        }
    }

    // Compute missing labels (those not covered by explicit branches)
    let mut missing: Vec<String> = u
        .variants
        .keys()
        .filter(|lbl| !covered.contains(*lbl))
        .cloned()
        .collect();
    missing.sort(); // Sorting for deterministic error messages (for tests)

    // Exhaustiveness: if no default, all labels must be covered
    if default.is_none() && !missing.is_empty() {
        return Err(EvalError::NonExhaustiveCase {
            ty: u.name.clone(),
            missing,
        });
    }

    // Default compatibility based on remaining labels
    // Determine whether remaining labels require a payload function, and if so,
    // ensure all remaining payload argument types are identical.
    if let Some(defexpr) = default {
        let mut remaining_payload_arg: Option<Type> = None;

        // track whether the default would have to be both a value and a function
        let mut has_simple_remaining = false;
        let mut has_payload_remaining = false;

        for lbl in &missing {
            match u.variants.get(lbl) {
                Some(None) => {
                    has_simple_remaining = true;
                }
                Some(Some(arg_ty)) => {
                    has_payload_remaining = true;

                    // Must have the same payload type!
                    match &remaining_payload_arg {
                        None => remaining_payload_arg = Some(arg_ty.clone()),
                        Some(prev) if prev == arg_ty => {}
                        Some(_) => return Err(EvalError::BadDefaultClause),
                    }
                }
                None => {} // can't happen => already validated labels exist
            }
        }

        // mixed set => default cannot be both a value and a function
        if has_simple_remaining && has_payload_remaining {
            return Err(EvalError::BadDefaultClause);
        }

        // If remaining labels include payload labels, default MUST be a lambda
        // with parameter type matching that payload type.
        if let Some(arg_ty) = remaining_payload_arg {
            match defexpr {
                Expr::Lambda { param, .. } => {
                    let got_param = lower_type_expr(&param.ty);
                    // Resolve alias on both sides so ex: Str vs String doesn't break
                    let exp = resolve_named(&arg_ty, resolved).unwrap_or(arg_ty);
                    let got = resolve_named(&got_param, resolved).unwrap_or(got_param);

                    if exp != got {
                        return Err(EvalError::BadDefaultClause);
                    }
                }
                _ => return Err(EvalError::DefaultNotAFunction),
            }
        }
    }

    // Choose explicit label branch or default
    let branch_expr = branches
        .iter()
        .find(|b| b.name == label)
        .map(|b| b.expr.as_ref());

    let chosen: &Expr = match branch_expr {
        Some(e) => e,
        None => default.ok_or_else(|| EvalError::MissingCaseBranch(label.clone()))?,
    };

    // Evaluate chosen branch
    let res = match payload {
        None => {
            // simple label => branch evaluates to a value
            eval(chosen, env, venv, resolved, globals)?
        }
        Some(p) => {
            // payload label => branch must evaluate to a function; apply to payload
            let fun_v = eval(chosen, env, venv, resolved, globals)?;
            apply_to_payload(fun_v, *p, venv, resolved, globals)?
        }
    };

    // Dynamic "same result type" enforcement
    // Using case_id as can have multiple case expressions in a program and need to keep their constraints separate
    // Stable for the duration of evaluation (unique per case node)
    // Storing the first observed result type and then
    // require all later executions of that case node to return the same type
    let got = resolve_named(&type_of_value(&res), resolved).unwrap_or(type_of_value(&res));
    match globals.case_result_ty.get(&case_id) {
        None => {
            globals.case_result_ty.insert(case_id, got);
        }
        Some(expected) => {
            if *expected != got {
                return Err(EvalError::CaseResultMismatch {
                    expected: expected.clone(),
                    got,
                });
            }
        }
    }

    Ok(res)
}

// Function application with runtime type checks
fn apply_to_payload(
    fun_v: Value,
    arg_v: Value,
    venv: &ValueEnv,
    resolved: &ResolvedTypeEnv,
    globals: &mut Globals,
) -> Result<Value, EvalError> {
    // If payload branch returns a non-function => Error
    let (param, param_ty, ret_ty, body, mut clos_env) = match fun_v {
        Value::Closure {
            param,
            param_ty,
            ret_ty,
            body,
            env,
        } => (param, param_ty, ret_ty, body, env),
        _ => return Err(EvalError::NotAFunction),
    };

    // Payload type checked against lambda parameter annotation
    ensure_type(&arg_v, &param_ty, resolved)?;
    clos_env.insert(param, arg_v);

    // Evaluate body
    let res = eval(&body, &mut clos_env, venv, resolved, globals)?;

    // If closure created via coerce.. with return-type constraint (ret_ty), enforce it
    if let Some(rt) = ret_ty {
        ensure_type(&res, &rt, resolved)?;
    }
    Ok(res)
}

fn coerce_value_to_type(
    v: Value,
    expected: &Type,
    resolved: &ResolvedTypeEnv,
) -> Result<Value, EvalError> {
    let exp_res = resolve_named(expected, resolved).unwrap_or(expected.clone());

    match exp_res {
        Type::Arrow(dom, cod) => {
            match v {
                Value::Closure {
                    param,
                    param_ty,
                    ret_ty: _,
                    body,
                    env,
                } => {
                    // Check domain matches closure param_ty (correct arg type)
                    let dom_res = resolve_named(&dom, resolved).unwrap_or(*dom);
                    let param_res = resolve_named(&param_ty, resolved).unwrap_or(param_ty.clone());
                    if dom_res != param_res {
                        return Err(EvalError::RuntimeTypeMismatch {
                            expected: Type::Arrow(Box::new(dom_res), cod),
                            got: Type::Arrow(
                                Box::new(param_res),
                                Box::new(Type::Named("<unknown>".into())),
                            ),
                        });
                    }

                    Ok(Value::Closure {
                        param,
                        param_ty,
                        ret_ty: Some(*cod), // enforce result type on application
                        body,
                        env,
                    })
                }
                other => Err(EvalError::RuntimeTypeMismatch {
                    expected: Type::Arrow(dom, cod),
                    got: type_of_value(&other),
                }),
            }
        }

        Type::Tuple(ts) => {
            let Value::Tuple(xs) = v else {
                return Err(EvalError::RuntimeTypeMismatch {
                    expected: Type::Tuple(ts),
                    got: type_of_value(&v),
                });
            };
            // Check arity
            if xs.len() != ts.len() {
                return Err(EvalError::RuntimeTypeMismatch {
                    expected: Type::Tuple(ts),
                    got: Type::Tuple(xs.iter().map(type_of_value).collect()),
                });
            }
            let mut out = Vec::with_capacity(xs.len());
            // Zips (puts) the two iterators together
            for (x, t) in xs.into_iter().zip(ts.iter()) {
                // Coerce each component
                out.push(coerce_value_to_type(x, t, resolved)?);
            }
            Ok(Value::Tuple(out))
        }

        // For nominal structs/unions we just need nominal identity after resolving aliases
        Type::Struct(s_exp) => {
            let got = type_of_value(&v);
            let got_res = resolve_named(&got, resolved).unwrap_or(got);
            match got_res {
                // Verify nominal identity
                Type::Struct(s_got) if s_got.name == s_exp.name => Ok(v),
                _ => Err(EvalError::RuntimeTypeMismatch {
                    expected: Type::Struct(s_exp),
                    got: got_res,
                }),
            }
        }

        Type::Union(u_exp) => {
            let got = type_of_value(&v);
            // Resolving aliases
            let got_res = resolve_named(&got, resolved).unwrap_or(got);
            match got_res {
                // Verify nominal identity
                Type::Union(u_got) if u_got.name == u_exp.name => Ok(v),
                _ => Err(EvalError::RuntimeTypeMismatch {
                    expected: Type::Union(u_exp),
                    got: got_res,
                }),
            }
        }

        // Named should have been resolved; fallback to old ensure_type behavior
        other => {
            ensure_type(&v, &other, resolved)?;
            Ok(v)
        }
    }
}
