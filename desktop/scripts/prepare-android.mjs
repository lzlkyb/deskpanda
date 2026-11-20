import { copyFile, mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

// 2026-11-20 自 pastePanda 迁入（手机端迁移 S7），差异：
// - 去掉 easytier 备用承载全部逻辑（deskpanda 无 underlay feature）；
// - 去掉知识库分享 intent-filter / knowledge_share_out file_paths（deskpanda 无知识库）；
// - 包名 com.pastepanda.app → com.pastepanda.deskpanda（与 identifier 一致）。
const permissions = ["CAMERA", "FOREGROUND_SERVICE", "FOREGROUND_SERVICE_SPECIAL_USE", "POST_NOTIFICATIONS", "WAKE_LOCK", "ACCESS_WIFI_STATE", "ACCESS_NETWORK_STATE", "REQUEST_INSTALL_PACKAGES"];
const service = `        <service android:name=".RcSessionForegroundService" android:exported="false" android:foregroundServiceType="specialUse">
            <property android:name="android.app.PROPERTY_SPECIAL_USE_FGS_SUBTYPE" android:value="remote_desktop_session_keepalive" />
        </service>`;

/** 仅补本项目的原生声明，保留 Tauri 生成的 Activity、intent 和其他插件。 */
export function patchAndroidManifest(manifest) {
  if (!/<application\b/.test(manifest) || !manifest.includes("</application>")) {
    throw new Error("AndroidManifest.xml 缺少 application，无法恢复原生插件");
  }
  const declared = new Set([...manifest.matchAll(/<uses-permission\b[^>]*android:name\s*=\s*["']([^"']+)["']/g)].map(match => match[1]));
  const missing = permissions.filter(name => !declared.has(`android.permission.${name}`))
    .map(name => `    <uses-permission android:name="android.permission.${name}" />`);
  if (!/<uses-feature\b[^>]*android:name\s*=\s*["']android\.hardware\.camera["']/.test(manifest)) {
    missing.push('    <uses-feature android:name="android.hardware.camera" android:required="false" />');
  }
  if (missing.length) manifest = manifest.replace(/([ \t]*)<application\b/, `${missing.join("\n")}\n\n$1<application`);
  if (!/<service\b[^>]*android:name\s*=\s*["'](?:\.RcSessionForegroundService|com\.pastepanda\.deskpanda\.RcSessionForegroundService)["']/.test(manifest)) {
    manifest = manifest.replace(/([ \t]*)<\/application>/, `${service}\n$1</application>`);
  }
  // Opt this activity into Android 13+ callbacks, including regenerated older templates.
  manifest = manifest.replace(/<activity\b[^>]*android:name=["'](?:\.MainActivity|com\.pastepanda\.deskpanda\.MainActivity)["'][^>]*>/, tag => {
    if (/android:enableOnBackInvokedCallback\s*=/.test(tag)) return tag.replace(/android:enableOnBackInvokedCallback\s*=\s*["'][^"']*["']/, 'android:enableOnBackInvokedCallback="true"');
    return tag.replace(/\/?>$/, end => ` android:enableOnBackInvokedCallback="true"${end}`);
  });
  if (!manifest.includes('androidx.core.content.FileProvider')) {
    manifest = manifest.replace('</application>', `<provider android:name="androidx.core.content.FileProvider" android:authorities="\${applicationId}.fileprovider" android:exported="false" android:grantUriPermissions="true">
            <meta-data android:name="android.support.FILE_PROVIDER_PATHS" android:resource="@xml/file_paths" />
        </provider></application>`);
  }
  return manifest;
}

/** gen/android/app/build.gradle.kts 不在版本控制里，`tauri android init` 重新生成后
 *  也不会带 androidx.core——ApkInstallerPlugin 的 FileProvider 编译需要它。幂等补一行。 */
export function patchAndroidGradle(gradle) {
  const anchor = gradle.match(/.*implementation\("androidx\.appcompat:appcompat[^"]*"\).*/);
  if (!anchor) throw new Error("build.gradle.kts 里找不到 appcompat 依赖行，无法注入 androidx.core");
  if (!/androidx\.core[:/]/.test(gradle)) gradle = gradle.replace(anchor[0], `${anchor[0]}\n    implementation("androidx.core:core:1.13.1")`);
  // Predictive callbacks require Activity 1.8+; retain newer generated dependencies.
  if (/implementation\("androidx\.activity:activity-ktx:[^"]*"\)/.test(gradle)) {
    gradle = gradle.replace(/implementation\("androidx\.activity:activity-ktx:([^".]*)\.([^".]*)\.([^"]*)"\)/, (line, major, minor, patch) => {
      const current = [Number(major), Number(minor), Number(patch)];
      if (current[0] > 1 || (current[0] === 1 && (current[1] > 10 || (current[1] === 10 && current[2] >= 1)))) return line;
      return 'implementation("androidx.activity:activity-ktx:1.10.1")';
    });
  } else gradle = gradle.replace(anchor[0], `${anchor[0]}\n    implementation("androidx.activity:activity-ktx:1.10.1")`);
  return gradle;
}

/** gen/ 不入库：每次 Android 构建前从版本控制中的唯一源恢复。 */
export async function prepareAndroid(root) {
  const main = path.join(root, "src-tauri/gen/android/app/src/main");
  const manifestPath = path.join(main, "AndroidManifest.xml");
  let manifest;
  try { manifest = await readFile(manifestPath, "utf8"); }
  catch (error) {
    if (error.code === "ENOENT") throw new Error("请先运行 npm run tauri android init，再执行 Android 构建");
    throw error;
  }
  const patched = patchAndroidManifest(manifest);
  const java = path.join(main, "java/com/pastepanda/deskpanda");
  await mkdir(java, { recursive: true });
  for (const name of ["MainActivity.kt", "RcSessionDisplay.kt", "RcKeepalivePlugin.kt", "RcSessionForegroundService.kt", "ApkInstallerPlugin.kt", "IrohNetworkPlugin.kt", "ExportChooserPlugin.kt", "ReceivedFileActions.kt", "MobileInteractionPlugin.kt"]) {
    await copyFile(path.join(root, "src-tauri/android", name), path.join(java, name));
  }
  if (patched !== manifest) await writeFile(manifestPath, patched, "utf8");
  const xml = path.join(main, "res/xml");
  await mkdir(xml, { recursive: true });
  const filePaths = path.join(xml, "file_paths.xml");
  let paths;
  try { paths = await readFile(filePaths, "utf8"); }
  catch (error) { if (error.code !== "ENOENT") throw error; paths = '<paths xmlns:android="http://schemas.android.com/apk/res/android"><cache-path name="my_cache_images" path="." /></paths>'; }
  // 接收文件导出目录（ReceivedFileActions 经 FileProvider 授权单文件）
  if (!paths.includes('name="received_file_out"')) {
    paths = paths.replace('</paths>', '<cache-path name="received_file_out" path="received-file-out/" /></paths>');
    await writeFile(filePaths, paths, "utf8");
  }
  const gradlePath = path.join(root, "src-tauri/gen/android/app/build.gradle.kts");
  const gradle = await readFile(gradlePath, "utf8");
  const gradlePatched = patchAndroidGradle(gradle);
  if (gradlePatched !== gradle) await writeFile(gradlePath, gradlePatched, "utf8");
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await prepareAndroid(path.resolve(path.dirname(fileURLToPath(import.meta.url)), ".."));
  console.log("[android-prepare] 原生源码与清单已同步");
}
