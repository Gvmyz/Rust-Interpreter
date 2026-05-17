
use std::fs;
use std::path::PathBuf;

fn write_prelude(tmp: &tempfile::TempDir) -> PathBuf {
    let p = tmp.path().join("prelude.tifl");
    // Put `prelude.tifl` in your crate root and keep this path:
    fs::write(&p, include_str!("../prelude.tifl")).unwrap();
    p
}

#[test]
fn prelude_bool_ops() {
    let tmp = tempfile::tempdir().unwrap();
    let prelude = write_prelude(&tmp);

    let out = tifl_interpreter::run_from_files(&prelude, "not Bool[true]").unwrap();
    assert_eq!(out, "Bool[false]");

    let out = tifl_interpreter::run_from_files(&prelude, "and Bool[true] Bool[false]").unwrap();
    assert_eq!(out, "Bool[false]");

    let out = tifl_interpreter::run_from_files(&prelude, "or Bool[false] Bool[true]").unwrap();
    assert_eq!(out, "Bool[true]");

    let out = tifl_interpreter::run_from_files(&prelude, "ifBool Bool[false] Bool[true] Bool[false]").unwrap();
    assert_eq!(out, "Bool[false]");
}

 #[test]
fn prelude_pairs_and_access() {
    let tmp = tempfile::tempdir().unwrap();
    let prelude = write_prelude(&tmp);

    let out = tifl_interpreter::run_from_files(&prelude, "fst (mkPair Bool[true] Bool[false])").unwrap();
    assert_eq!(out, "Bool[true]");

    let out = tifl_interpreter::run_from_files(&prelude, "(Bool[true],Bool[false]).2").unwrap();
    assert_eq!(out, "Bool[false]");
}


#[test]
fn prelude_option_case_payload() {
    let tmp = tempfile::tempdir().unwrap();
    let prelude = write_prelude(&tmp);

    let out = tifl_interpreter::run_from_files(&prelude, "unwrapOr OptBool[none] Bool[true]").unwrap();
    assert_eq!(out, "Bool[true]");

    let out = tifl_interpreter::run_from_files(&prelude, "unwrapOr OptBool[some=Bool[false]] Bool[true]").unwrap();
    assert_eq!(out, "Bool[false]");
    
    // payload case: if some(x) => not x, else => false
    let out = tifl_interpreter::run_from_files(
        &prelude,
        "OptBool[some=Bool[false]][some=\\x:Bool.not x,none=Bool[false]]",
    ).unwrap();
    assert_eq!(out, "Bool[true]"); 
}

#[test]
fn prelude_string_constant_prints_stably() {
    let tmp = tempfile::tempdir().unwrap();
    let prelude = write_prelude(&tmp);

    let out = tifl_interpreter::run_from_files(&prelude, "abStr").unwrap();
    assert_eq!(out, "Str[cons=(Char[a],Str[cons=(Char[b],Str[empty])])]");
}

#[test]
fn intentional_type_errors_are_reported() {
    let tmp = tempfile::tempdir().unwrap();
    let prelude = write_prelude(&tmp);

    // Applying not to a Pair is a runtime type error
    let err = tifl_interpreter::run_from_files(
        &prelude,
        "not Pair{a=Bool[true],b=Bool[false]}",
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("type error") || msg.contains("RuntimeTypeMismatch"));

    // Missing struct field
    let err = tifl_interpreter::run_from_files(&prelude, "Pair{a=Bool[true]}").unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("missing field") && msg.contains("Pair"));

    // Wrong union payload type
    let err = tifl_interpreter::run_from_files(
        &prelude,
        "OptBool[some=Pair{a=Bool[true],b=Bool[false]}]",
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("type error") || msg.contains("RuntimeTypeMismatch"));

    // Default used for payload label but default is not a function
    let err = tifl_interpreter::run_from_files(
        &prelude,
        "OptBool[some=Bool[true]][none=Bool[false] | Bool[false]]",
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("default") || msg.contains("function"));
} 