use assert_cmd::Command;

fn bin() -> Command {
    Command::cargo_bin("fulgur-chart").unwrap()
}

#[test]
fn renders_bar_to_svg_stdout() {
    let spec = r#"{"type":"bar","data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]}}"#;
    let out = bin()
        .args(["render", "-", "-o", "-"])
        .write_stdin(spec)
        .assert()
        .success();
    let s = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(s.starts_with("<svg"));
    assert!(s.trim_end().ends_with("</svg>"));
}

#[test]
fn renders_bar_to_file() {
    let dir = tempfile_dir();
    let spec_path = dir.join("spec.json");
    let out_path = dir.join("out.svg");
    std::fs::write(
        &spec_path,
        r#"{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]}}"#,
    )
    .unwrap();
    bin()
        .args([
            "render",
            spec_path.to_str().unwrap(),
            "-o",
            out_path.to_str().unwrap(),
        ])
        .assert()
        .success();
    let s = std::fs::read_to_string(&out_path).unwrap();
    assert!(s.starts_with("<svg"));
}

#[test]
fn invalid_json_exits_1() {
    bin()
        .args(["render", "-", "-o", "-"])
        .write_stdin("{ not json")
        .assert()
        .failure()
        .code(1);
}

#[test]
fn strict_unknown_key_exits_2() {
    let spec = r#"{"type":"bar","data":{"labels":[],"datasets":[]},"wat":1}"#;
    bin()
        .args(["render", "-", "-o", "-", "--strict"])
        .write_stdin(spec)
        .assert()
        .failure()
        .code(2);
    // 非strictは成功
    bin()
        .args(["render", "-", "-o", "-"])
        .write_stdin(spec)
        .assert()
        .success();
}

#[test]
fn png_not_supported_exits_3() {
    let spec = r#"{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]}}"#;
    bin()
        .args(["render", "-", "-o", "-", "--format", "png"])
        .write_stdin(spec)
        .assert()
        .failure()
        .code(3);
}

#[test]
fn missing_input_file_exits_1() {
    bin()
        .args(["render", "/nonexistent/xyz.json", "-o", "-"])
        .assert()
        .failure()
        .code(1);
}

// tempdir ヘルパ: tempfile クレートを使わず std::env::temp_dir に一意ディレクトリを作る
fn tempfile_dir() -> std::path::PathBuf {
    let base = std::env::temp_dir().join(format!("fulgur_chart_cli_test_{}", std::process::id()));
    std::fs::create_dir_all(&base).unwrap();
    base
}
