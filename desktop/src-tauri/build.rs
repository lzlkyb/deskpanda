fn main() {
    // ─── Windows：comctl32 v6 manifest 的唯一供给点 ───
    //
    // 背景（2026-10-27 实测定位，0xc0000139 STATUS_ENTRYPOINT_NOT_FOUND）：
    // 一旦测试代码构建 mock 应用（tauri::test::mock_builder + generate_context!），
    // 链接器会把 wry 运行时（tauri-runtime-wry）的原生对话框代码拉进测试二进制，
    // 其导入表含 comctl32.dll!TaskDialogIndirect —— 该入口点只存在于 comctl32 v6。
    // 没有 manifest 时加载器给 comctl32 v5 ⇒ 进程启动即崩，连 libtest 的
    // "running tests" 都打不出来。
    //
    // tauri-build 默认会替 bin 目标嵌这份 manifest（tauri-winres 经
    // embed_resource::compile 只发 `cargo:rustc-link-arg-bins`），测试目标不是 bin，
    // 拿不到。而 `cargo:rustc-link-arg-tests` 也到不了 **lib 单测二进制**
    // （实测：tests/ 集成测试二进制 manifest=1，lib 单测二进制 manifest=0；
    // cargo 对 `--lib --test` 不应用 link-arg-tests）。
    //
    // 唯一能同时覆盖「app bin / lib 单测 / tests/ 集成测试」的口径是**裸**
    // `cargo:rustc-link-arg`。但裸 link-arg 也覆盖 bin ⇒ 若 tauri-build 仍然自己
    // 嵌一份，app bin 就会有两份 RT_MANIFEST（ID 1），链接直接失败：
    //   CVTRES : fatal error CVT1100: 资源重复。类型: MANIFEST，名称: 1，语言: 0x0409
    //   LINK   : fatal error LNK1123: 转换到 COFF 期间失败: 文件无效或损坏
    // （2026-11-20 实测。）
    //
    // 所以：让 tauri-build **不**生成 manifest（`new_without_app_manifest`），
    // manifest 统一由本文件末尾那一份 lib 供给，每个二进制恰好一份。
    // 两份内容完全一致（tauri 默认 manifest == 下面的 Common-Controls 6 依赖），
    // 不是功能裁剪。
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest()),
    )
    .unwrap_or_else(|e| {
        eprintln!("tauri_build::try_build failed: {e:#}");
        std::process::exit(1);
    });

    if std::env::var("CARGO_CFG_WINDOWS").is_ok() {
        let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
        let manifest_path = std::path::Path::new(&out_dir).join("tests-comctl32.manifest");
        std::fs::write(
            &manifest_path,
            r#"<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity
        type="win32"
        name="Microsoft.Windows.Common-Controls"
        version="6.0.0.0"
        processorArchitecture="*"
        publicKeyToken="6595b64144ccf1df"
        language="*"
      />
    </dependentAssembly>
  </dependency>
</assembly>
"#,
        )
        .expect("failed to write tests manifest");

        // RC 语法：资源名 1、类型 24（RT_MANIFEST）、引用上面的 manifest。
        // 写绝对路径，避开 rc.exe 相对路径解析的不确定性；
        // 反斜杠必须翻倍——RC 字符串里 \t 是 TAB 等转义，单杠会被吃掉（RC2135）。
        let rc_path = std::path::Path::new(&out_dir).join("tests-comctl32.rc");
        let manifest_in_rc = manifest_path.display().to_string().replace('\\', "\\\\");
        std::fs::write(
            &rc_path,
            format!("#pragma code_page(65001)\n1 24 \"{}\"\n", manifest_in_rc),
        )
        .expect("failed to write tests rc");

        // 裸 link-arg（compile_for_everything ⇒ cargo:rustc-link-arg）覆盖全部产物：
        // app bin、lib 单测二进制、tests/ 集成测试。
        // 上方已让 tauri-build 不再生成 manifest，故每个二进制都只有这一份，
        // 不会撞 CVT1100。
        // ⚠️ 若将来给 tauri-build 恢复默认 manifest，这里必须同步二选一，
        //    否则 app bin 两份 RT_MANIFEST ⇒ LNK1123（2026-11-20 实测）。
        embed_resource::compile_for_everything(&rc_path, embed_resource::NONE)
            .manifest_required()
            .unwrap();
    }
}
