#!/usr/bin/env bash
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${FURNACE:-$ROOT/target/debug/furnace}"
TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT
export FURNACE_CONFIG_DIR="$TMPDIR/furnace-config"

fail() {
    echo "FAIL: $1" >&2
    exit 1
}

if [ ! -x "$BIN" ]; then
    echo "furnace binary not found at $BIN" >&2
    echo "build with: cargo build --bin furnace" >&2
    exit 2
fi

# version output
actual="$($BIN -version 2>&1)"
[ "$actual" = "Furnace Alpha-7" ] || fail "-version output mismatch: '$actual'"

# help output
help_actual="$($BIN -help 2>&1)"
printf '%s\n' "$help_actual" | grep -q "Furnace Alpha-7" || fail "-help is missing banner"
printf '%s\n' "$help_actual" | grep -q "Furnace compile <file>.anvil \[linux|windows\]" || fail "-help is missing compile usage"
printf '%s\n' "$help_actual" | grep -q "Furnace build <project>.blower" || fail "-help is missing build usage"
printf '%s\n' "$help_actual" | grep -q "Furnace run <file>.anvil" || fail "-help is missing run usage"
printf '%s\n' "$help_actual" | grep -q "Furnace backend <native|cranelift>" || fail "-help is missing backend usage"
printf '%s\n' "$help_actual" | grep -q "Available backends:" || fail "-help is missing backend list"
printf '%s\n' "$help_actual" | grep -q "Furnace new <APP_TYPE> -n <NAME>" || fail "-help is missing new usage"
printf '%s\n' "$help_actual" | grep -q "Furnace update" || fail "-help is missing update usage"
printf '%s\n' "$help_actual" | grep -q "console" || fail "-help is missing console application type"

# backend command
native_backend_out="$($BIN backend native 2>&1)" || fail "native backend command failed"
[ "$native_backend_out" = "Default backend set to native" ] || fail "native backend output mismatch: '$native_backend_out'"
[ "$(cat "$FURNACE_CONFIG_DIR/config")" = 'backend = "native"' ] || fail "native backend was not persisted"
cranelift_backend_out="$($BIN backend cranelift 2>&1)" || fail "cranelift backend command failed"
[ "$cranelift_backend_out" = "Default backend set to cranelift" ] || fail "cranelift backend output mismatch: '$cranelift_backend_out'"
[ "$(cat "$FURNACE_CONFIG_DIR/config")" = 'backend = "cranelift"' ] || fail "cranelift backend was not persisted"
if $BIN backend unknown >/dev/null 2>&1; then
    fail "unknown backend should fail"
fi

# new console project
PROJECT_DIR="$TMPDIR/Project"
new_out="$(cd "$TMPDIR" && "$BIN" new console -n Project 2>&1)" || fail "new console project failed"
[ -d "$PROJECT_DIR" ] || fail "new did not create project directory"
[ -f "$PROJECT_DIR/Project.blower" ] || fail "new did not create project file"
[ -f "$PROJECT_DIR/src/Main.anvil" ] || fail "new did not create project source"
expected_source=$'Open Nunction Main()\n{\n}\n'
actual_source="$(cat "$PROJECT_DIR/src/Main.anvil")"$'\n'
[ "$actual_source" = "$expected_source" ] || fail "new generated unexpected source"

generated_compile_out="$(cd "$TMPDIR" && "$BIN" build Project/Project.blower 2>&1)" || fail "generated project did not build"
printf '%s\n' "$generated_compile_out" | grep -q "Build successful!" || fail "generated project compile missing success"
[ -x "$PROJECT_DIR/build/Project" ] || fail "generated project executable missing"

# different project name
OTHER_DIR="$TMPDIR/AnotherProject"
(cd "$TMPDIR" && "$BIN" new console -n AnotherProject >/dev/null 2>&1) || fail "new rejected a second valid name"
[ -f "$OTHER_DIR/AnotherProject.blower" ] || fail "new hardcoded the project name"

# existing directory must not be overwritten
EXISTING_DIR="$TMPDIR/Existing"
mkdir "$EXISTING_DIR"
printf 'keep\n' > "$EXISTING_DIR/marker.txt"
if (cd "$TMPDIR" && "$BIN" new console -n Existing >/dev/null 2>&1); then
    fail "new overwrote an existing directory"
fi
[ -f "$EXISTING_DIR/marker.txt" ] || fail "new removed existing directory contents"
[ ! -f "$EXISTING_DIR/Existing.blower" ] || fail "new created a file in an existing directory"

# build rejects non-project inputs
if "$BIN" build "$PROJECT_DIR/src/Main.anvil" >/dev/null 2>&1; then
    fail "build with non-blower file should fail"
fi

# invalid new usage
if (cd "$TMPDIR" && "$BIN" new >/dev/null 2>&1); then
    fail "new without an app type should fail"
fi
if (cd "$TMPDIR" && "$BIN" new console >/dev/null 2>&1); then
    fail "new without a name should fail"
fi
if (cd "$TMPDIR" && "$BIN" new console -n "" >/dev/null 2>&1); then
    fail "new with an empty name should fail"
fi
if (cd "$TMPDIR" && "$BIN" new unknown -n Unknown >/dev/null 2>&1); then
    fail "new with an unknown app type should fail"
fi
[ ! -e "$TMPDIR/Unknown" ] || fail "unknown app type created a project"

# compile success
SRC="$TMPDIR/ok.anvil"
cat > "$SRC" <<'EOF'
Open Nunction Main()
{
    Print("ok-from-cli");
}
EOF

OUT="$TMPDIR/ok"
compile_out="$(cd "$TMPDIR" && "$BIN" compile "$SRC" linux 2>&1)"
printf '%s\n' "$compile_out" | grep -q "Compiling" || fail "compile output missing compile notice"
printf '%s\n' "$compile_out" | grep -q "Linking" || fail "compile output missing linking notice"
printf '%s\n' "$compile_out" | grep -q "Build successful!" || fail "compile output missing success"
[ -x "$OUT" ] || fail "expected compiled executable at $OUT"

# run success
run_out="$($BIN run "$SRC" 2>&1)"
printf '%s\n' "$run_out" | grep -q "ok-from-cli" || fail "run output missing program output"

# invalid extension
if $BIN compile "$TMPDIR/invalid.txt" linux >/dev/null 2>&1; then
    fail "compile with non-anvil file should fail"
fi

# Windows cross-compilation through both supported target spellings.
WINDOWS_POSITIONAL_SRC="$TMPDIR/windows-positional.anvil"
cat > "$WINDOWS_POSITIONAL_SRC" <<'EOF'
Open Int Main()
{
    Return 7;
}
EOF
if ! (cd "$TMPDIR" && "$BIN" compile "$WINDOWS_POSITIONAL_SRC" windows --backend native >/dev/null 2>&1); then
    fail "positional Windows cross-compilation failed"
fi
WINDOWS_POSITIONAL_OUT="$TMPDIR/windows-positional.exe"
[ -f "$WINDOWS_POSITIONAL_OUT" ] || fail "positional Windows compilation did not create .exe output"
[ "$(od -An -t x1 -N2 "$WINDOWS_POSITIONAL_OUT" | tr -d ' ')" = "4d5a" ] || fail "positional Windows output is not PE/MZ"

WINDOWS_FLAG_SRC="$TMPDIR/windows-flag.anvil"
cat > "$WINDOWS_FLAG_SRC" <<'EOF'
Open Int Main()
{
    Return 9;
}
EOF
if ! (cd "$TMPDIR" && "$BIN" compile "$WINDOWS_FLAG_SRC" --windows --backend native >/dev/null 2>&1); then
    fail "--windows cross-compilation failed"
fi
WINDOWS_FLAG_OUT="$TMPDIR/windows-flag.exe"
[ -f "$WINDOWS_FLAG_OUT" ] || fail "--windows compilation did not create .exe output"
[ "$(od -An -t x1 -N2 "$WINDOWS_FLAG_OUT" | tr -d ' ')" = "4d5a" ] || fail "--windows output is not PE/MZ"

# Typed errors use the direct native backend and must continue to work inside Switch.
ERROR_SRC="$TMPDIR/typed-errors.anvil"
cat > "$ERROR_SRC" <<'EOF'
Open Int Main()
{
    Do
    {
        Switch (0)
        {
            Case 0 -> Throw Error.InvalidValue("zero is invalid");
            Base -> Print("FAIL: typed error base");
        }
    }
    Catch Error.InvalidValue => Problem
    {
        Print(Problem);
    }
    Finally
    {
        Print("cleanup");
    }
    Return 0;
}
EOF
if ! (cd "$TMPDIR" && "$BIN" compile "$ERROR_SRC" linux --backend native >/dev/null 2>&1); then
    fail "native typed-error compilation failed"
fi
error_actual="$($TMPDIR/typed-errors 2>&1)"
error_expected=$'zero is invalid\ncleanup'
[ "$error_actual" = "$error_expected" ] || fail "typed-error output mismatch: '$error_actual'"

# Truly unsupported positional targets still fail.
if $BIN compile "$SRC" macos >/dev/null 2>&1; then
    fail "compile with unsupported platform should fail"
fi

# fake file path
if $BIN run "$TMPDIR/missing.anvil" >/dev/null 2>&1; then
    fail "run should fail for missing file"
fi

# broken program should fail strongly
BROKEN="$TMPDIR/broken.anvil"
cat > "$BROKEN" <<'EOF'
Open Nunction Main()
{
    Print(unknown_var);
}
EOF
if $BIN compile "$BROKEN" linux >/dev/null 2>&1; then
    fail "compile should fail for invalid Sydrogen code"
fi

echo "CLI tests passed"
