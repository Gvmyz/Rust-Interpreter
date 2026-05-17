use tifl_interpreter::{error::EvalError, run};

fn ok(program: &str, expr: &str) -> String {
    run(program, expr).unwrap()
}

fn err(program: &str, expr: &str) -> tifl_interpreter::error::EvalError {
    match tifl_interpreter::run(program, expr) {
        Ok(_) => panic!("expected error"),
        Err(e) => {
            let rep = e.downcast_ref::<tifl_interpreter::error::EvalError>().unwrap();
            rep.clone()
        }
    }
}

#[test]
fn value_def_and_application() {
    let program = r#"Bool=[true,false], id=\x:Bool.x"#;
    assert_eq!(ok(program, r#"id Bool[true]"#), "Bool[true]");
}

#[test]
fn lambda_and_capture() {
    // makeAdder = \x:Int.\y:Int.+ x y  (capture x)
    let program = r#"
        Bool=[true,false],
        Int=[0,i:Int,d:Int,nan],
        int0=Int[0],
        incr=\x:Int.x[nan=Int[nan],0=Int[i=int0],i=\v:Int.Int[i=(incr v)],d=\v:Int.v],
        +=\x:Int.\y:Int.x[nan=Int[nan],0=y,i=\v:Int.+ v (incr y),d=\v:Int.+ v (Int[d=y])],
        makeAdder=\x:Int.\y:Int.+ x y
    "#;
    // 1 + 1
    let expr = r#"(makeAdder (Int[i=int0])) (Int[i=int0])"#;
    // should be Int[i=Int[i=Int[0]]] if your + and incr are right; keep it looser:
    let out = ok(program, expr);
    
    assert_eq!(out, "Int[i=Int[i=Int[0]]]");
}

#[test]
fn tuple_and_projection() {
    let program = r#"Bool=[true,false]"#;
    assert_eq!(ok(program, r#"(Bool[true], Bool[false]).1"#), "Bool[true]");
    assert_eq!(ok(program, r#"(Bool[true], Bool[false]).2"#), "Bool[false]");
}

#[test]
fn struct_type_and_constructor_and_field_access() {
    let program = r#"
        Bool=[true,false],
        PairBool={fst:Bool,snd:Bool},
        mk=\a:Bool.\b:Bool.PairBool{fst=a,snd=b}
    "#;

    assert_eq!(ok(program, r#"(mk Bool[true] Bool[false]).fst"#), "Bool[true]");
    assert_eq!(ok(program, r#"(mk Bool[true] Bool[false]).snd"#), "Bool[false]");
}

#[test]
fn union_ctor_errors_are_enforced() {
    let program = r#"Bool=[true,false], OptBool=[none,some:Bool]"#;
    let e1 = err(program, r#"OptBool[some]"#);
    // some requires payload
    assert!(matches!(e1, EvalError::MissingPayload { label } if label == "some"));
    let e2 = err(program, r#"OptBool[none=Bool[true]]"#);
    // none forbids payload
    assert!(matches!(e2, EvalError::UnexpectedPayload { label } if label == "none"));
}

#[test]
fn case_on_union_no_payload_branches() {
    let program = r#"
        Bool=[true,false],
        not=\b:Bool.b[true=Bool[false],false=Bool[true]]
    "#;
    assert_eq!(ok(program, r#"not Bool[true]"#), "Bool[false]");
    assert_eq!(ok(program, r#"not Bool[false]"#), "Bool[true]");
}

#[test]
fn case_on_union_payload_branch_applies_function() {
    let program = r#"
        Bool=[true,false],
        OptBool=[none,some:Bool],
        getOr=\d:Bool.\o:OptBool.o[
            none=d,
            some=\b:Bool.b
        ]
    "#;

    assert_eq!(ok(program, r#"getOr Bool[false] OptBool[none]"#), "Bool[false]");
    assert_eq!(ok(program, r#"getOr Bool[false] OptBool[some=Bool[true]]"#), "Bool[true]");
}

#[test]
fn case_default_value_used_for_remaining_simple_labels() {
    let program = r#"
        Bool=[true,false],
        U=[a,b,c],
        f=\x:U.x[a=Bool[true] | Bool[false]]
    "#;
    assert_eq!(ok(program, r#"f U[a]"#), "Bool[true]");
    assert_eq!(ok(program, r#"f U[b]"#), "Bool[false]");
    assert_eq!(ok(program, r#"f U[c]"#), "Bool[false]");
}

#[test]
fn case_default_function_used_for_remaining_payload_labels() {
    let program = r#"
        Bool=[true,false],
        Int=[0,i:Int,d:Int,nan],
        int0=Int[0],
        U=[a:Int,b:Int,c:Int],
        f=\x:U.x[a=\n:Int.Bool[true] | \n:Int.Bool[false]]
    "#;
    assert_eq!(ok(program, r#"f U[a=int0]"#), "Bool[true]");
    assert_eq!(ok(program, r#"f U[b=int0]"#), "Bool[false]");
} 

//
// ----------- ERROR TESTS (nontrivial runtime/type errors) -----------
//
 
#[test]
fn error_unbound_variable() {
    let program = r#"Bool=[true,false]"#;
    let msg = err(program, r#"x"#);
    assert!(matches!(msg, EvalError::UnboundVar(_)));
}


#[test]
fn error_apply_non_function() {
    let program = r#"Bool=[true,false]"#;
    let msg = err(program, r#"Bool[true] Bool[false]"#);
    assert!(matches!(msg, EvalError::NotAFunction));
}

#[test]
fn error_tuple_index_oob() {
    let program = r#"Bool=[true,false]"#;
    let msg = err(program, r#"(Bool[true], Bool[false]).3"#);
    println!("{}", msg);
    assert!(matches!(msg, EvalError::TupleIndexOob { .. }));
}

#[test]
fn error_unknown_struct_field_access() {
    let program = r#"
        Bool=[true,false],
        PairBool={fst:Bool,snd:Bool}
    "#;
    let msg = err(program, r#"PairBool{fst=Bool[true],snd=Bool[false]}.nope"#);
    assert!(matches!(msg, EvalError::UnknownField { .. }));
}

#[test]
fn error_struct_ctor_missing_field() {
    let program = r#"
        Bool=[true,false],
        PairBool={fst:Bool,snd:Bool}
    "#;
    let msg = err(program, r#"PairBool{fst=Bool[true]}"#);
    assert!(matches!(msg, EvalError::MissingStructField { .. }));
}

#[test]
fn error_struct_ctor_unknown_field() {
    let program = r#"
        Bool=[true,false],
        PairBool={fst:Bool,snd:Bool}
    "#;
    let msg = err(program, r#"PairBool{fst=Bool[true],snd=Bool[false],x=Bool[true]}"#);
    assert!(matches!(msg, EvalError::UnknownStructInitField { .. }));
}

#[test]
fn error_struct_ctor_duplicate_field() {
    let program = r#"
        Bool=[true,false],
        PairBool={fst:Bool,snd:Bool}
    "#;
    let msg = err(program, r#"PairBool{fst=Bool[true],fst=Bool[false],snd=Bool[true]}"#);
    assert!(matches!(msg, EvalError::DuplicateStructField { .. }));
}

#[test]
fn error_union_ctor_missing_payload() {
    let program = r#"
        Bool=[true,false],
        OptBool=[none,some:Bool]
    "#;
    let msg = err(program, r#"OptBool[some]"#);
    assert!(matches!(msg, EvalError::MissingPayload { .. }));
}

#[test]
fn error_union_ctor_unexpected_payload() {
    let program = r#"
        Bool=[true,false],
        OptBool=[none,some:Bool]
    "#;
    let msg = err(program, r#"OptBool[none=Bool[true]]"#);
    assert!(matches!(msg, EvalError::UnexpectedPayload { .. }));
}

#[test]
fn error_case_on_non_union() {
    let program = r#"Bool=[true,false]"#;

    // scrutinee is a tuple -> not a union
    let e = err(program, r#"(Bool[true], Bool[false])[x=Bool[true]]"#);
    assert!(matches!(e, EvalError::CaseOnNonUnion));
}

#[test]
fn error_non_exhaustive_case_no_default() {
    let program = r#"
        Bool=[true,false],
        U=[a,b,c],
        f=\x:U.x[a=Bool[true]]
    "#;
    let msg = err(program, r#"f U[b]"#);
    println!("{}", msg);
    assert!(matches!(msg, EvalError::NonExhaustiveCase { .. }));
}

#[test]
fn error_bad_default_clause_mixed_payload_and_simple_remaining() {
    let program = r#"
        Bool=[true,false],
        Int=[0,i:Int,d:Int,nan],
        int0=Int[0],
        U=[a,b:Int,c],
        f=\x:U.x[a=Bool[true] | Bool[false]]
    "#;
    // missing labels include b:Int (payload) and c (simple) => default can't serve both
    let msg = err(program, r#"f U[b=int0]"#);
    assert!(matches!(msg, EvalError::BadDefaultClause));
}

#[test]
fn error_default_not_a_function_when_payload_remaining() {
    let program = r#"
        Bool=[true,false],
        Int=[0,i:Int,d:Int,nan],
        int0=Int[0],
        U=[a:Int,b:Int],
        f=\x:U.x[a=\n:Int.Bool[true] | Bool[false]]
    "#;
    // default is Bool[false] but remaining labels require a function
    let msg = err(program, r#"f U[b=int0]"#);
    assert!(matches!(msg, EvalError::DefaultNotAFunction | EvalError::BadDefaultClause));
}

#[test]
fn error_runtime_type_mismatch_in_application() {
    use tifl_interpreter::types::Type;
    // idInt expects Int, but we pass Bool
    let program = r#"
        Bool=[true,false],
        Int=[0,i:Int,d:Int,nan],
        idInt=\x:Int.x
    "#;
    let msg = err(program, r#"idInt Bool[true]"#);
    println!("{}", msg);
    assert!(matches!(msg, EvalError::RuntimeTypeMismatch { expected: Type::Named(ref exp), got: Type::Union(ref u) } if exp == "Int" && u.name == "Bool" ));
}

#[test]
fn structural_alias_behaves_like_underlying_type() {
    let program = r#"
        Bool=[true,false],
        A=Bool,
        idA=\x:A.x
    "#;

    let out = tifl_interpreter::run(program, r#"idA Bool[true]"#).unwrap();
    assert_eq!(out, "Bool[true]");
}

#[test]
fn nominal_types_are_not_structurally_equal() {
    let program = r#"
        U=[a],
        V=[a],
        idV=\x:V.x
    "#;

    let e = err(program, r#"idV U[a]"#);
    assert!(matches!(e, EvalError::RuntimeTypeMismatch { .. }));
}

#[test]
fn nominal_structs_are_not_interchangeable() {
    let program = r#"
        Bool=[true,false],
        S={x:Bool},
        T={x:Bool},
        idT=\p:T.p
    "#;

    let e = err(program, r#"idT S{x=Bool[true]}"#);
    assert!(matches!(e, EvalError::RuntimeTypeMismatch { .. }));
}