// exe 图标嵌入（assets/ring.ico 由代码生成的圆环图案）
fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS")
        .map(|v| v == "windows")
        .unwrap_or(false)
    {
        winres::WindowsResource::new()
            .set_icon("assets/ring.ico")
            .set_manifest_file("assets/app.manifest")
            .compile()
            .expect("winres 资源编译失败");
    }
}
