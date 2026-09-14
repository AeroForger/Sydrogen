use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

use furnace::backend::{compile_to_elf, compile_to_pe};
use furnace::parser::parse_program;
use furnace::semantic::analyze;

fn checked(source: &str) -> furnace::ast::Program {
    let program = parse_program(source).unwrap();
    analyze(&program).unwrap();
    program
}

fn compile_and_run(source: &str) -> (i32, String) {
    let bytes = compile_to_elf(&checked(source)).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let executable = temp.path().join("errors");
    fs::write(&executable, bytes).unwrap();
    let mut permissions = fs::metadata(&executable).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&executable, permissions).unwrap();
    let output = Command::new(executable).output().unwrap();
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
    )
}

#[test]
fn parses_typed_error_nodes_and_custom_binding() {
    let program = checked(
        r#"Open Nunction Main() {
            Do { Throw Error.InvalidValue("bad value"); }
            Catch Error.InvalidValue => Err { Print(Err); }
            Finally { Print("done"); }
        }"#,
    );
    let furnace::ast::Statement::FunctionDecl(main) = &program.statements[0] else {
        panic!("expected Main");
    };
    let furnace::ast::Statement::Do(node) = &main.body[0] else {
        panic!("expected Do");
    };
    assert_eq!(node.catches[0].binding, "Err");
    assert_eq!(node.catches[0].error_type.name(), "Error.InvalidValue");
    assert!(node.finally_body.is_some());
}

#[test]
fn catches_explicit_throw_and_runs_finally() {
    let (status, output) = compile_and_run(
        r#"Open Int Main() {
            Do { Throw Error.InvalidValue("bad value"); }
            Catch Error.InvalidValue => Err { Print(Err); }
            Finally { Print("finished"); }
            Return 0;
        }"#,
    );
    assert_eq!(status, 0, "output before termination: {output:?}");
    assert_eq!(output, "bad value\nfinished\n");
}

#[test]
fn do_finally_runs_on_success() {
    let (status, output) = compile_and_run(
        r#"Open Int Main() {
            Do { Print("work"); }
            Finally { Print("cleanup"); }
            Return 0;
        }"#,
    );
    assert_eq!(status, 0);
    assert_eq!(output, "work\ncleanup\n");
}

#[test]
fn finally_runs_before_return_leaves_a_protected_block() {
    let (status, output) = compile_and_run(
        r#"Open Int Main() {
            Do { Return 17; }
            Finally { Print("before return"); }
        }"#,
    );
    assert_eq!(status, 17);
    assert_eq!(output, "before return\n");
}

#[test]
fn generic_and_multiple_catches_use_first_match() {
    let (_, output) = compile_and_run(
        r#"Open Int Main() {
            Do { Throw Error.Overflow("too large"); }
            Catch Error.InvalidValue => Wrong { Print("wrong"); }
            Catch Error.Overflow => Exact { Print(Exact); }
            Catch Error => Any { Print("generic"); }
            Return 0;
        }"#,
    );
    assert_eq!(output, "too large\n");
}

#[test]
fn division_by_zero_is_catchable() {
    let (status, output) = compile_and_run(
        r#"Open Int Main() {
            Do { Int Result = 10 / 0; Print(Result); }
            Catch Error.DivisionByZero => Err { Print(Err); }
            Return 0;
        }"#,
    );
    assert_eq!(status, 0);
    assert_eq!(output, "division by zero\n");
}

#[test]
fn nested_unmatched_errors_propagate_and_run_finally() {
    let (_, output) = compile_and_run(
        r#"Open Int Main() {
            Do {
                Do { Throw Error.InvalidValue("outer handles"); }
                Catch Error.IO => Inner { Print("wrong"); }
                Finally { Print("inner finally"); }
            }
            Catch Error.InvalidValue => Outer { Print(Outer); }
            Finally { Print("outer finally"); }
            Return 0;
        }"#,
    );
    assert_eq!(output, "inner finally\nouter handles\nouter finally\n");
}

#[test]
fn errors_propagate_across_function_calls() {
    let (_, output) = compile_and_run(
        r#"Nunction RaiseIssue() { Throw Error.IO("disk unavailable"); }
        Open Int Main() {
            Do { RaiseIssue(); }
            Catch Error.IO => Problem { Print(Problem); }
            Return 0;
        }"#,
    );
    assert_eq!(output, "disk unavailable\n");
}

#[test]
fn uncaught_error_prints_type_and_message_and_exits_cleanly() {
    let (status, output) =
        compile_and_run(r#"Open Int Main() { Throw Error.InvalidValue("uncaught value"); }"#);
    assert_eq!(status, 1);
    assert_eq!(output, "Error.InvalidValue: uncaught value\n");
}

#[test]
fn semantic_rules_reject_invalid_handlers_and_throw_values() {
    let cases = [
        (
            "Open Nunction Main() { Do { Int X = 1; } }",
            "at least one Catch or Finally",
        ),
        (
            r#"Open Nunction Main() {
                Do { Throw Error.IO("x"); }
                Catch Error => Any { }
                Catch Error.IO => Io { }
            }"#,
            "unreachable",
        ),
        (
            r#"Open Nunction Main() {
                Do { Throw Error.IO("x"); }
                Catch Error.IO => A { }
                Catch Error.IO => B { }
            }"#,
            "duplicate Catch",
        ),
        (
            "Open Nunction Main() { Throw Error.InvalidValue(123); }",
            "requires a Weld message",
        ),
        (
            "Open Nunction Main() { Throw Error.Unknown(\"x\"); }",
            "invalid error type 'Error.Unknown'",
        ),
        (
            r#"Open Nunction Main() {
                Do { Int X = 1; }
                Catch Error.Unknown => Err { }
            }"#,
            "invalid error type 'Error.Unknown'",
        ),
        (
            r#"Open Nunction Main() {
                Do { Throw Error.IO("x"); }
                Catch Error.IO => Local { }
                Print(Local);
            }"#,
            "Undefined variable: Local",
        ),
    ];
    for (source, expected) in cases {
        let program = parse_program(source).unwrap();
        let error = match analyze(&program) {
            Err(error) => error.to_string(),
            Ok(()) => panic!("semantic analysis accepted: {source}"),
        };
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn malformed_error_syntax_is_a_parse_error() {
    for source in [
        "Open Nunction Main() { Do { } Catch Error.IO Err { } }",
        "Open Nunction Main() { Catch Error => Err { } }",
    ] {
        assert!(
            parse_program(source).is_err(),
            "unexpectedly parsed {source}"
        );
    }
}

#[test]
fn native_elf_and_pe_compile_error_control_flow() {
    let program = checked(
        r#"Open Int Main() {
            Do { Throw Error.InvalidValue("handled"); }
            Catch Error.InvalidValue => Err { Int Value = 4; }
            Finally { Int Cleanup = 1; }
            Return 0;
        }"#,
    );
    assert_eq!(&compile_to_elf(&program).unwrap()[..4], b"\x7fELF");
    assert_eq!(&compile_to_pe(&program).unwrap()[..2], b"MZ");
}

#[test]
fn cranelift_reports_error_handling_as_unsupported() {
    let program = checked(
        r#"Open Nunction Main() {
            Do { Throw Error.IO("no io"); }
            Catch Error.IO => Err { }
        }"#,
    );
    let temp = tempfile::tempdir().unwrap();
    let error = furnace::codegen::compile(&program, &temp.path().join("error.o"), true)
        .unwrap_err()
        .to_string();
    assert!(error.contains("not yet supported by the Cranelift backend"));

    let mixed = checked(
        r#"Open Float Main() {
            Do { Throw Error.IO("no io"); }
            Catch Error.IO => Err { }
            Return 1.0;
        }"#,
    );
    let error = compile_to_elf(&mixed).unwrap_err().to_string();
    assert!(error.contains("cannot be combined with floats"));
}
