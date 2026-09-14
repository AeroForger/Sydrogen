use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};

use furnace::ast::{Statement, SwitchBranch};
use furnace::backend::{compile_to_elf, compile_to_pe};
use furnace::parser::parse_program;
use furnace::semantic::analyze;

fn checked(source: &str) -> furnace::ast::Program {
    let program = parse_program(source).unwrap();
    analyze(&program).unwrap();
    program
}

fn compile_and_run(source: &str, stdin: &str) -> (i32, String) {
    let bytes = compile_to_elf(&checked(source)).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let executable = temp.path().join("switch-test");
    fs::write(&executable, bytes).unwrap();
    let mut permissions = fs::metadata(&executable).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&executable, permissions).unwrap();
    let mut child = Command::new(executable)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
    )
}

#[test]
fn parser_preserves_every_case_form_in_switch_ast() {
    let program = checked(
        r#"Open Nunction Main() {
            Switch (2) {
                Case 1 { Print("block"); }
                Case 2 -> Print("arrow");
                Case -> 3: Print("alternate");
                Case 4 or 5 { Print("multiple block"); }
                Case 6 or 7 -> Print("multiple arrow");
                Base -> Print("base");
            }
        }"#,
    );
    let Statement::FunctionDecl(main) = &program.statements[0] else {
        panic!("expected Main");
    };
    let Statement::Switch(node) = &main.body[0] else {
        panic!("expected Switch");
    };
    assert_eq!(node.branches.len(), 6);
    let SwitchBranch::Case(multiple) = &node.branches[3] else {
        panic!("expected multiple-value Case");
    };
    assert_eq!(multiple.values.len(), 2);
    assert!(matches!(node.branches.last(), Some(SwitchBranch::Base(_))));
}

#[test]
fn matches_first_middle_last_and_base_without_fallthrough() {
    for (selector, expected) in [(1, 11), (3, 23), (5, 35), (9, 49)] {
        let source = format!(
            r#"Open Int Main() {{
                Int Result = 0;
                Switch ({selector}) {{
                    Case 1 -> Result = 11;
                    Case 2 or 3 {{ Result = 23; }}
                    Case 5 -> Result = 35;
                    Base {{ Result = 49; }}
                }}
                Return Result;
            }}"#
        );
        assert_eq!(compile_and_run(&source, "").0, expected);
    }
}

#[test]
fn switch_without_base_is_a_noop_when_nothing_matches() {
    let (status, _) = compile_and_run(
        r#"Open Int Main() {
            Int Result = 8;
            Switch (99) { Case 1 -> Result = 1; }
            Result = Result + 2;
            Return Result;
        }"#,
        "",
    );
    assert_eq!(status, 10);
}

#[test]
fn selector_is_evaluated_once() {
    let (status, _) = compile_and_run(
        r#"Open Int Main() {
            Switch (Input(Int)) {
                Case 1 -> Return 11;
                Case 2 -> Return 22;
                Base -> Return 33;
            }
        }"#,
        "2\n",
    );
    assert_eq!(status, 22);
}

#[test]
fn nested_switches_and_code_after_switch_execute() {
    let (status, _) = compile_and_run(
        r#"Open Int Main() {
            Int Result = 0;
            Switch (2) {
                Case 2 {
                    Switch (4) { Case 4 -> Result = 40; Base -> Result = 1; }
                }
            }
            Result = Result + 2;
            Return Result;
        }"#,
        "",
    );
    assert_eq!(status, 42);
}

#[test]
fn switch_and_typed_errors_share_existing_propagation() {
    let (_, output) = compile_and_run(
        r#"Open Int Main() {
            Do {
                Switch (0) {
                    Case 0 -> Throw Error.InvalidValue("zero");
                    Base -> Print("wrong");
                }
            }
            Catch Error.InvalidValue => ERROR { Print(ERROR); }
            Finally { Print("finished"); }

            Switch (1) {
                Case 1 {
                    Do { Throw Error.IO("inside"); }
                    Catch Error.IO => INNER { Print(INNER); }
                }
            }
            Return 0;
        }"#,
        "",
    );
    assert_eq!(output, "zero\nfinished\ninside\n");
}

#[test]
fn catch_arrow_remains_distinct_from_case_arrow() {
    checked(
        r#"Open Int Main() {
            Do {
                Switch (1) { Case -> 1: Throw Error.IO("io"); }
            }
            Catch Error.IO => Problem { Print(Problem); }
            Return 0;
        }"#,
    );
}

#[test]
fn semantic_analysis_rejects_invalid_switches() {
    let cases = [
        (
            "Open Nunction Main() { Switch (1) { Case 1 {} Case 1 {} } }",
            "duplicate Case value",
        ),
        (
            "Open Nunction Main() { Switch (1) { Case 2 or 3 {} Case 3 {} } }",
            "duplicate Case value",
        ),
        (
            "Open Nunction Main() { Switch (1) { Base {} Base {} } }",
            "only one Base",
        ),
        (
            "Open Nunction Main() { Switch (1) { Base {} Case 2 {} } }",
            "Base must be the final branch",
        ),
        (
            "Open Nunction Main() { Switch (1) { Case \"one\" {} } }",
            "Case type mismatch",
        ),
    ];
    for (source, expected) in cases {
        let program = parse_program(source).unwrap();
        let error = analyze(&program).unwrap_err().to_string();
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn malformed_switch_and_case_syntax_are_parse_errors() {
    for source in [
        "Open Nunction Main() { Switch 1 { Case 1 {} } }",
        "Open Nunction Main() { Switch (1) { Case {} } }",
        "Open Nunction Main() { Switch (1) { Case 1 -> } }",
        "Open Nunction Main() { Switch (1) { Base Print(1); } }",
    ] {
        assert!(
            parse_program(source).is_err(),
            "unexpectedly parsed: {source}"
        );
    }
}

#[test]
fn native_elf_and_pe_compile_switch_control_flow() {
    let program = checked(
        r#"Open Int Main() {
            Int Result = 0;
            Switch (2) { Case 1 -> Result = 1; Case 2 or 3 -> Result = 7; Base -> Result = 9; }
            Return Result;
        }"#,
    );
    assert_eq!(&compile_to_elf(&program).unwrap()[..4], b"\x7fELF");
    assert_eq!(&compile_to_pe(&program).unwrap()[..2], b"MZ");
}

#[test]
fn cranelift_compiles_switch_control_flow() {
    let program = checked(
        r#"Open Int Main() {
            Float Value = 2.0;
            Switch (Value) { Case 1.0 -> Return 1; Case 2.0 -> Return 2; Base -> Return 3; }
        }"#,
    );
    let temp = tempfile::tempdir().unwrap();
    furnace::codegen::compile(&program, &temp.path().join("switch.o"), true).unwrap();
}
