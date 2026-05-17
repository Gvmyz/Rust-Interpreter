use miette::Result;
use tifl_interpreter::{ast::*, error::{ParseError, TiflError}, lexer::{Lexer, Token}, parse_program_src, parser::Parser};

fn parse_prog(src: &str) -> Program {
    let tokens = Lexer::new(src)
        .collect::<Result<Vec<Token>, TiflError>>()
        .expect("lexing of src");

    let mut p = Parser::new(tokens);
    p.run_parsing().expect("Paring of tokens")
}

fn parse_prog_error(src: &str) -> ParseError {
    let tokens = Lexer::new(src)
        .collect::<Result<Vec<Token>, TiflError>>()
        .expect("lexing of src");

    let mut p = Parser::new(tokens);
    p.run_parsing().expect_err("expected parser error")
}

fn v(name: &str) -> Expr {
    Expr::Atom(Atom::Var(name.to_string()))
}

fn app(fun: Expr, arg: Expr) -> Expr {
    Expr::Application {
            fun: Box::new(fun),
            arg: Box::new(arg),
    }
}

#[test]
fn prog_single_vdef_lambda() {
    let prog = parse_prog(r#"id=\x:T.x"#);

    assert_eq!(
        prog,
        Program {
            defs: vec![Def::Value(ValueDef {
                name: "id".into(),
                expr: Box::new(Expr::Lambda {
                    param: Decl {
                        name: "x".into(),
                        ty: TypeExpr::Named("T".into())
                    },
                    body: Box::new(v("x")),
                }),
            })]
        }
    );
}

#[test]
fn prog_multiple_defs_commas() {
    let prog = parse_prog(r#"id=\x:T.x,k=\x:T.\y:U.x"#);

    assert_eq!(prog.defs.len(), 2);
    assert!(matches!(prog.defs[0], Def::Value(_)));
    assert!(matches!(prog.defs[1], Def::Value(_)));
}

#[test]
fn def_include() {
    let prog = parse_prog(r#"@prelude.tifl"#);
    assert_eq!(
        prog,
        Program {
            defs: vec![Def::Include(IncludeDef {
                file_name: "prelude.tifl".into()
            })]
        }
    );
}

#[test]
fn def_include_then_value() {
    let prog = parse_prog(r#"@prelude.tifl,id=\x:T.x"#);
    assert_eq!(prog.defs.len(), 2);
    assert!(matches!(prog.defs[0], Def::Include(_)));
    assert!(matches!(prog.defs[1], Def::Value(_)));
}

#[test]
fn typedef_nominal_struct() {
    let prog = parse_prog(r#"Pair={a:A,b:B}"#);

    assert_eq!(
        prog,
        Program {
            defs: vec![Def::Type(TypeDef {
                name: "Pair".into(),
                rhs: TypeRhs::Nominal(NominalType::StructType(vec![
                    Decl {
                        name: "a".into(),
                        ty: TypeExpr::Named("A".into())
                    },
                    Decl {
                        name: "b".into(),
                        ty: TypeExpr::Named("B".into())
                    },
                ]))
            })]
        }
    );
}

#[test]
fn typedef_nominal_union() {
    let prog = parse_prog(r#"Opt=[none,some:A]"#);

    assert_eq!(
        prog,
        Program {
            defs: vec![Def::Type(TypeDef {
                name: "Opt".into(),
                rhs: TypeRhs::Nominal(NominalType::UnionType(vec![
                    Elem {
                        name: "none".into(),
                        elem_type: None
                    },
                    Elem {
                        name: "some".into(),
                        elem_type: Some(TypeExpr::Named("A".into()))
                    },
                ]))
            })]
        }
    );
}

#[test]
fn typedef_structural_arrow_right_assoc() {
    let prog = parse_prog(r#"Curry=T->U->V"#);

    // T -> (U -> V)
    let expected = TypeExpr::Arrow(
        Box::new(TypeExpr::Named("T".into())),
        Box::new(TypeExpr::Arrow(
            Box::new(TypeExpr::Named("U".into())),
            Box::new(TypeExpr::Named("V".into())),
        )),
    );

    assert_eq!(
        prog,
        Program {
            defs: vec![Def::Type(TypeDef {
                name: "Curry".into(),
                rhs: TypeRhs::Structural(expected),
            })]
        }
    );
}

#[test]
fn typedef_tuple_type() {
    let prog = parse_prog(r#"Tup=(A,B)"#);

    assert_eq!(
        prog,
        Program {
            defs: vec![Def::Type(TypeDef {
                name: "Tup".into(),
                rhs: TypeRhs::Structural(TypeExpr::Tuple(vec![
                    TypeExpr::Named("A".into()),
                    TypeExpr::Named("B".into()),
                ]))
            })]
        }
    );
}


#[test]
fn vexp_application_chain_left_assoc() {
    let prog = parse_prog(r#"e=f a b"#);

    // ((f a) b)
    let expected = app(app(v("f"), v("a")), v("b"));

    assert_eq!(
        prog,
        Program {
            defs: vec![Def::Value(ValueDef {
                name: "e".into(),
                expr: Box::new(expected),
            })]
        }
    );
}

#[test]
fn val_tuple_paren() {
    let prog = parse_prog(r#"t=(a,b,c)"#);

    assert_eq!(
        prog,
        Program {
            defs: vec![Def::Value(ValueDef {
                name: "t".into(),
                expr: Box::new(Expr::Atom(Atom::Tuple(vec![v("a"), v("b"), v("c")]))),
            })]
        }
    );
}

#[test]
fn val_access_chain() {
    let prog = parse_prog(r#"x=a.b.c"#);

    let expected_atom = Atom::Access {
        base: Box::new(Atom::Access {
            base: Box::new(Atom::Var("a".into())),
            field: "b".into(),
        }),
        field: "c".into(),
    };

    assert_eq!(
        prog,
        Program {
            defs: vec![Def::Value(ValueDef {
                name: "x".into(),
                expr: Box::new(Expr::Atom(expected_atom)),
            })]
        }
    );
}

#[test]
fn val_case_without_default() {
    let prog = parse_prog(r#"r=u[x=a,y=b]"#);

    let expected = Atom::Case {
        scrutinee: Box::new(Atom::Var("u".into())),
        branches: vec![
            ValueDef {
                name: "x".into(),
                expr: Box::new(v("a")),
            },
            ValueDef {
                name: "y".into(),
                expr: Box::new(v("b")),
            },
        ],
        default: None,
    };

    assert_eq!(
        prog,
        Program {
            defs: vec![Def::Value(ValueDef {
                name: "r".into(),
                expr: Box::new(Expr::Atom(expected)),
            })]
        }
    );
}


#[test]
fn val_case_with_default() {
    let prog = parse_prog(r#"r=u[x=a|z]"#);

    let expected = Atom::Case {
        scrutinee: Box::new(Atom::Var("u".into())),
        branches: vec![ValueDef {
            name: "x".into(),
            expr: Box::new(v("a")),
        }],
        default: Some(Box::new(v("z"))),
    };

    assert_eq!(
        prog,
        Program {
            defs: vec![Def::Value(ValueDef {
                name: "r".into(),
                expr: Box::new(Expr::Atom(expected)),
            })]
        }
    );
}

#[test]
fn fval_typed_union_label_spec() {
    let prog = parse_prog(r#"x=Opt[none]"#);

    let expected = Expr::Atom(Atom::Typed {
        ty_name: "Opt".into(),
        spec: Spec::UnionLabel("none".into()),
    });

    assert_eq!(
        prog,
        Program {
            defs: vec![Def::Value(ValueDef {
                name: "x".into(),
                expr: Box::new(expected),
            })]
        }
    );
}

#[test]
fn fval_typed_union_field_spec() {
    let prog = parse_prog(r#"x=Opt[some=a]"#);

    let expected = Expr::Atom(Atom::Typed {
        ty_name: "Opt".into(),
        spec: Spec::UnionField(ValueDef {
            name: "some".into(),
            expr: Box::new(v("a")),
        }),
    });

    assert_eq!(
        prog,
        Program {
            defs: vec![Def::Value(ValueDef {
                name: "x".into(),
                expr: Box::new(expected),
            })]
        }
    );
}

#[test]
fn fval_typed_struct_fields_spec() {
    let prog = parse_prog(r#"p=Pair{a=x,b=y}"#);

    let expected = Expr::Atom(Atom::Typed {
        ty_name: "Pair".into(),
        spec: Spec::StructFields(vec![
            ValueDef {
                name: "a".into(),
                expr: Box::new(v("x")),
            },
            ValueDef {
                name: "b".into(),
                expr: Box::new(v("y")),
            },
        ]),
    });

    assert_eq!(
        prog,
        Program {
            defs: vec![Def::Value(ValueDef {
                name: "p".into(),
                expr: Box::new(expected),
            })]
        }
    );
}

#[test]
fn typed_struct_empty_fields_allowed_if_you_support_it() {
    // If your grammar allows empty struct spec {}, keep this test.
    // If you decide it must contain at least one vdef, then flip to parse_prog_err.
    let prog = parse_prog(r#"u=Unit{}"#);

    assert_eq!(
        prog.defs.len(),
        1,
        "should parse one value def"
    );
}

// ---------- Negative tests (parser errors) ----------

#[test]
fn error_missing_equal_in_vdef() {
    let err = parse_prog_error(r#"x \y:T.y"#);
    match err {
        ParseError::Msg(_) => {}
    }
}

#[test]
fn error_unterminated_tuple() {
    let err = parse_prog_error(r#"x=(a,b"#);
    match err {
        ParseError::Msg(_) => {}
    }
}

#[test]
fn error_case_missing_rbracket() {
    let err = parse_prog_error(r#"x=u[a=b"#);
    match err {
        ParseError::Msg(_) => {}
    }
}

#[test]
fn error_typed_missing_spec() {
    // "Opt" alone isn't a <val>, and <tname><spec> requires a spec
    let err = parse_prog_error(r#"x=Opt"#);
    match err {
        ParseError::Msg(_) => {}
    }
}

#[test]
fn include_parses_relative_paths() {
    let program_src = r#"@./std/prelude.tifl, id=\x:T.x"#;
    let prog = parse_program_src(program_src).unwrap();
    match &prog.defs[0] {
        Def::Include(inc) => assert_eq!(inc.file_name, "./std/prelude.tifl"),
        _ => panic!("expected include def"),
    }
}