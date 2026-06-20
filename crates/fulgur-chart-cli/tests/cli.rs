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
fn renders_bar_to_png_file() {
    let dir = tempfile_dir();
    let out = dir.join("out.png");
    let spec = r#"{"type":"bar","data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]}}"#;
    bin()
        .args([
            "render",
            "-",
            "-o",
            out.to_str().unwrap(),
            "--format",
            "png",
        ])
        .write_stdin(spec)
        .assert()
        .success();
    let bytes = std::fs::read(&out).unwrap();
    assert_eq!(&bytes[0..4], &[0x89, b'P', b'N', b'G']);
}

#[test]
fn renders_to_png_stdout() {
    // -o - かつ --format png のとき、stdout 先頭 4 バイトが PNG シグネチャ。
    let spec = r#"{"type":"bar","data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]}}"#;
    let out = bin()
        .args(["render", "-", "-o", "-", "--format", "png"])
        .write_stdin(spec)
        .assert()
        .success();
    let bytes = &out.get_output().stdout;
    assert_eq!(&bytes[0..4], &[0x89, b'P', b'N', b'G']);
}

#[test]
fn missing_input_file_exits_1() {
    bin()
        .args(["render", "/nonexistent/xyz.json", "-o", "-"])
        .assert()
        .failure()
        .code(1);
}

/// 同梱フォントの絶対パス(CLI クレートから見た相対で解決)。
const BUNDLED_FONT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../fulgur-chart/assets/fonts/NotoSansJP-Regular.otf"
);

#[test]
fn font_flag_with_bundled_font_succeeds() {
    let spec = r#"{"type":"bar","data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]}}"#;
    let out = bin()
        .args(["render", "-", "-o", "-", "--font", BUNDLED_FONT])
        .write_stdin(spec)
        .assert()
        .success();
    let s = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(s.starts_with("<svg"));
}

#[test]
fn font_flag_missing_file_fails() {
    let spec = r#"{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]}}"#;
    bin()
        .args(["render", "-", "-o", "-", "--font", "/no/such.ttf"])
        .write_stdin(spec)
        .assert()
        .failure();
}

/// 最小の Vega-Lite bar spec。
const MINIMAL_VEGALITE_BAR: &str = r#"{"mark":"bar","data":{"values":[{"c":"A","v":3},{"c":"B","v":5}]},"encoding":{"x":{"field":"c","type":"nominal"},"y":{"field":"v","type":"quantitative"}}}"#;

#[test]
fn renders_vegalite_spec() {
    let out = bin()
        .args(["render", "-", "-o", "-", "--dsl", "vegalite"])
        .write_stdin(MINIMAL_VEGALITE_BAR)
        .assert()
        .success();
    let s = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(s.starts_with("<svg"));
}

#[test]
fn vegalite_strict_unknown_key_exits_2() {
    let spec = r#"{"wat":1,"mark":"bar","data":{"values":[{"c":"A","v":3},{"c":"B","v":5}]},"encoding":{"x":{"field":"c","type":"nominal"},"y":{"field":"v","type":"quantitative"}}}"#;
    bin()
        .args(["render", "-", "-o", "-", "--dsl", "vegalite", "--strict"])
        .write_stdin(spec)
        .assert()
        .failure()
        .code(2);
}

#[test]
fn unknown_dsl_exits_1() {
    bin()
        .args(["render", "-", "-o", "-", "--dsl", "bogus"])
        .write_stdin(MINIMAL_BAR_A)
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

// バッチ用 tempdir: テスト名ごとに固定ディレクトリを使い、開始時に消してクリーンスレートにする。
fn batch_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("fulgur_batch_{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const MINIMAL_BAR_A: &str =
    r#"{"type":"bar","data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]}}"#;
const MINIMAL_BAR_B: &str =
    r#"{"type":"bar","data":{"labels":["x","y","z"],"datasets":[{"data":[3,4,5]}]}}"#;

#[test]
fn batch_renders_multiple_svgs() {
    let dir = batch_dir("batch_renders_multiple_svgs");
    let in_dir = dir.join("in");
    let out_dir = dir.join("out");
    std::fs::create_dir_all(&in_dir).unwrap();
    let a = in_dir.join("a.json");
    let b = in_dir.join("b.json");
    std::fs::write(&a, MINIMAL_BAR_A).unwrap();
    std::fs::write(&b, MINIMAL_BAR_B).unwrap();

    bin()
        .args([
            "render",
            a.to_str().unwrap(),
            b.to_str().unwrap(),
            "--out-dir",
            out_dir.to_str().unwrap(),
        ])
        .assert()
        .success();

    let sa = std::fs::read_to_string(out_dir.join("a.svg")).unwrap();
    let sb = std::fs::read_to_string(out_dir.join("b.svg")).unwrap();
    assert!(sa.starts_with("<svg"));
    assert!(sb.starts_with("<svg"));
}

#[test]
fn batch_renders_png() {
    let dir = batch_dir("batch_renders_png");
    let in_dir = dir.join("in");
    let out_dir = dir.join("out");
    std::fs::create_dir_all(&in_dir).unwrap();
    let a = in_dir.join("a.json");
    let b = in_dir.join("b.json");
    std::fs::write(&a, MINIMAL_BAR_A).unwrap();
    std::fs::write(&b, MINIMAL_BAR_B).unwrap();

    bin()
        .args([
            "render",
            a.to_str().unwrap(),
            b.to_str().unwrap(),
            "--out-dir",
            out_dir.to_str().unwrap(),
            "--format",
            "png",
        ])
        .assert()
        .success();

    let bytes = std::fs::read(out_dir.join("a.png")).unwrap();
    assert_eq!(&bytes[0..4], &[0x89, b'P', b'N', b'G']);
}

#[test]
fn batch_matches_single() {
    let dir = batch_dir("batch_matches_single");
    let in_dir = dir.join("in");
    let out_dir = dir.join("out");
    std::fs::create_dir_all(&in_dir).unwrap();
    let a = in_dir.join("a.json");
    std::fs::write(&a, MINIMAL_BAR_A).unwrap();

    // バッチ出力。
    bin()
        .args([
            "render",
            a.to_str().unwrap(),
            "--out-dir",
            out_dir.to_str().unwrap(),
        ])
        .assert()
        .success();
    let batch_bytes = std::fs::read(out_dir.join("a.svg")).unwrap();

    // 単一モード(stdout)出力。
    let single = bin()
        .args(["render", a.to_str().unwrap(), "-o", "-"])
        .assert()
        .success();
    let single_bytes = single.get_output().stdout.clone();

    // バッチと単一はバイト一致。
    assert_eq!(batch_bytes, single_bytes);
}

#[test]
fn batch_requires_out_dir_for_multiple() {
    let dir = batch_dir("batch_requires_out_dir_for_multiple");
    let a = dir.join("a.json");
    let b = dir.join("b.json");
    std::fs::write(&a, MINIMAL_BAR_A).unwrap();
    std::fs::write(&b, MINIMAL_BAR_B).unwrap();

    bin()
        .args(["render", a.to_str().unwrap(), b.to_str().unwrap()])
        .assert()
        .failure();
}

#[test]
fn out_dir_conflicts_with_output() {
    let dir = batch_dir("out_dir_conflicts_with_output");
    let in_dir = dir.join("in");
    let out_dir = dir.join("out");
    std::fs::create_dir_all(&in_dir).unwrap();
    let a = in_dir.join("a.json");
    std::fs::write(&a, MINIMAL_BAR_A).unwrap();

    bin()
        .args([
            "render",
            a.to_str().unwrap(),
            "--out-dir",
            out_dir.to_str().unwrap(),
            "-o",
            "x.svg",
        ])
        .assert()
        .failure();
}

#[test]
fn batch_rejects_stem_collision() {
    let dir = batch_dir("batch_rejects_stem_collision");
    let d1 = dir.join("d1");
    let d2 = dir.join("d2");
    let out_dir = dir.join("out");
    std::fs::create_dir_all(&d1).unwrap();
    std::fs::create_dir_all(&d2).unwrap();
    // 別ディレクトリの同名 stem (a.json) → 出力 a.<ext> が衝突 → 書き出し前に fail-fast。
    let a1 = d1.join("a.json");
    let a2 = d2.join("a.json");
    std::fs::write(&a1, MINIMAL_BAR_A).unwrap();
    std::fs::write(&a2, MINIMAL_BAR_A).unwrap();

    bin()
        .args([
            "render",
            a1.to_str().unwrap(),
            a2.to_str().unwrap(),
            "--out-dir",
            out_dir.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .code(1);
    // 衝突検出は create_dir_all より前なので、成果物も out_dir も作らない。
    assert!(!out_dir.join("a.svg").exists(), "部分出力を残さない");
}

#[test]
fn batch_rejects_stdin_before_any_output() {
    let dir = batch_dir("batch_rejects_stdin_before_any_output");
    let in_dir = dir.join("in");
    let out_dir = dir.join("out");
    std::fs::create_dir_all(&in_dir).unwrap();
    let valid = in_dir.join("valid.json");
    std::fs::write(&valid, MINIMAL_BAR_A).unwrap();

    // `valid.json -` の順でも、検証は出力前に行われ `-` で失敗し、部分出力を残さない。
    bin()
        .args([
            "render",
            valid.to_str().unwrap(),
            "-",
            "--out-dir",
            out_dir.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .code(1);
    assert!(
        !out_dir.join("valid.svg").exists(),
        "失敗時に部分成果物を残さない"
    );
}

#[test]
fn batch_render_error_leaves_no_partial_output() {
    let dir = batch_dir("batch_render_error_leaves_no_partial_output");
    let in_dir = dir.join("in");
    let out_dir = dir.join("out");
    std::fs::create_dir_all(&in_dir).unwrap();
    let good = in_dir.join("good.json");
    let bad = in_dir.join("bad.json");
    std::fs::write(&good, MINIMAL_BAR_A).unwrap();
    std::fs::write(&bad, "{ not valid json").unwrap();

    // good→bad の順でも、二相処理(全件レンダリング後に一括書き出し)により
    // 2 件目の JSON エラーで失敗し、good の成果物も書き出されない。
    bin()
        .args([
            "render",
            good.to_str().unwrap(),
            bad.to_str().unwrap(),
            "--out-dir",
            out_dir.to_str().unwrap(),
        ])
        .assert()
        .failure();
    assert!(
        !out_dir.join("good.svg").exists(),
        "レンダリング失敗時に先行成果物を残さない"
    );
}

#[test]
fn batch_preflight_blocks_when_output_is_directory() {
    let dir = batch_dir("batch_preflight_blocks_when_output_is_directory");
    let in_dir = dir.join("in");
    let out_dir = dir.join("out");
    std::fs::create_dir_all(&in_dir).unwrap();
    std::fs::create_dir_all(&out_dir).unwrap();
    // b.svg を「ディレクトリ」として作り、2 件目の出力先を塞ぐ。
    std::fs::create_dir_all(out_dir.join("b.svg")).unwrap();
    let a = in_dir.join("a.json");
    let b = in_dir.join("b.json");
    std::fs::write(&a, MINIMAL_BAR_A).unwrap();
    std::fs::write(&b, MINIMAL_BAR_A).unwrap();

    bin()
        .args([
            "render",
            a.to_str().unwrap(),
            b.to_str().unwrap(),
            "--out-dir",
            out_dir.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .code(3);
    // preflight で書き込み前に中止するため、先行する a.svg も書かれない。
    assert!(
        !out_dir.join("a.svg").exists(),
        "preflight 失敗時に部分出力を残さない"
    );
}

#[test]
fn schema_chartjs_is_valid_json() {
    let output = Command::cargo_bin("fulgur-chart")
        .unwrap()
        .args(["schema"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    let v: serde_json::Value = serde_json::from_str(&text).expect("not valid JSON");
    // $schema フィールドが存在する
    assert!(v.get("$schema").is_some(), "missing $schema");
    // oneOf / anyOf のいずれかが存在する（discriminated union）
    let has_union = v.get("oneOf").is_some() || v.get("anyOf").is_some();
    assert!(has_union, "expected union schema (oneOf or anyOf)");
}

#[test]
fn schema_vegalite_is_valid_json() {
    let output = Command::cargo_bin("fulgur-chart")
        .unwrap()
        .args(["schema", "--dsl", "vegalite"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    let _: serde_json::Value = serde_json::from_str(&text).expect("not valid JSON");
}

#[test]
fn schema_unknown_dsl_exits_1() {
    let output = Command::cargo_bin("fulgur-chart")
        .unwrap()
        .args(["schema", "--dsl", "unknown"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn auto_detect_chartjs_without_dsl_flag() {
    let spec = r#"{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[1]}]}}"#;
    let out = bin()
        .args(["render", "-", "-o", "-"])
        .write_stdin(spec)
        .assert()
        .success();
    let s = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(s.starts_with("<svg"));
}

#[test]
fn auto_detect_vegalite_without_dsl_flag() {
    let spec = r#"{"mark":"bar","data":{"values":[{"x":"A","y":1}]},"encoding":{"x":{"field":"x"},"y":{"field":"y"}}}"#;
    let out = bin()
        .args(["render", "-", "-o", "-"])
        .write_stdin(spec)
        .assert()
        .success();
    let s = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(s.starts_with("<svg"));
}

#[test]
fn auto_detect_unknown_spec_exits_1() {
    let spec = r#"{"labels":["A"],"values":[1]}"#;
    let out = bin()
        .args(["render", "-", "-o", "-"])
        .write_stdin(spec)
        .assert()
        .failure()
        .code(1);
    let stderr = String::from_utf8(out.get_output().stderr.clone()).unwrap();
    assert!(
        stderr.contains("auto-detect"),
        "stderr should contain 'auto-detect', got: {stderr}"
    );
}

// --- 入力上限（guard モジュールが CLI 経由で正しく動作することを確認）---

#[test]
fn oversized_width_exits_1() {
    // width が MAX_DIMENSION_PX (32768) を超えると exit 1。
    let spec = r#"{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]}}"#;
    let out = bin()
        .args(["render", "-", "-o", "-", "--width", "32769"])
        .write_stdin(spec)
        .assert()
        .failure()
        .code(1);
    let stderr = String::from_utf8(out.get_output().stderr.clone()).unwrap();
    assert!(
        stderr.contains("width"),
        "stderr should mention 'width', got: {stderr}"
    );
}

#[test]
fn oversized_height_exits_1() {
    // height が MAX_DIMENSION_PX (32768) を超えると exit 1。
    let spec = r#"{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]}}"#;
    let out = bin()
        .args(["render", "-", "-o", "-", "--height", "32769"])
        .write_stdin(spec)
        .assert()
        .failure()
        .code(1);
    let stderr = String::from_utf8(out.get_output().stderr.clone()).unwrap();
    assert!(
        stderr.contains("height"),
        "stderr should mention 'height', got: {stderr}"
    );
}

#[test]
fn zero_width_exits_1() {
    // width=0 (MIN_DIMENSION_PX 未満) も exit 1。
    let spec = r#"{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]}}"#;
    bin()
        .args(["render", "-", "-o", "-", "--width", "0"])
        .write_stdin(spec)
        .assert()
        .failure()
        .code(1);
}

// --- inspect サブコマンド（意味モデルを JSON で出力）---

#[test]
fn inspect_bar_emits_model_json() {
    // bar spec を inspect して JSON モデルを stdout に得る。
    let spec = r#"{"type":"bar","data":{"labels":["1月","2月","3月"],"datasets":[{"label":"売上","data":[120,200,150]}]}}"#;
    let out = bin()
        .args(["inspect", "-", "-o", "-"])
        .write_stdin(spec)
        .assert()
        .success();
    let bytes = out.get_output().stdout.clone();
    let v: serde_json::Value = serde_json::from_slice(&bytes).expect("valid JSON");
    assert_eq!(v["meta"]["type"], "bar");
    assert!(!v["series"].as_array().unwrap().is_empty());
    assert!(v["axes"]["y"]["ticks"].is_array());
}
