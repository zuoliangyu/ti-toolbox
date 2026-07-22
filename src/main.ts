import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { relaunch } from "@tauri-apps/plugin-process";
import { check } from "@tauri-apps/plugin-updater";
import "./styles.css";

type ProjectKind = "ccs" | "keil";
type AppMode = "convert" | "setup";

interface EnvironmentDiscovery {
  projectKind: ProjectKind;
  device: string;
  sdkPath: string | null;
  sdkVersion: string | null;
  packPath: string | null;
  packName: string | null;
  packVersion: string | null;
  packInstalled: boolean;
  packDownloadUrl: string | null;
  ccsPath: string | null;
  ccsExecutable: string | null;
  keilPath: string | null;
  keilExecutable: string | null;
  sysconfigPath: string | null;
  sysconfigExecutable: string | null;
  warnings: string[];
}

interface KeilEnvironmentDiscovery {
  sdkPath: string | null;
  sdkVersion: string | null;
  keilPath: string | null;
  keilExecutable: string | null;
  sysconfigPath: string | null;
  sysconfigExecutable: string | null;
  warnings: string[];
}

interface KeilSysConfigResult {
  changed: boolean;
  slot: number;
  title: string;
  updatedFiles: string[];
  backupFiles: string[];
}

interface ProjectFile {
  path: string;
  group: string;
  fileType: string;
}

interface ProjectInspection {
  kind: ProjectKind;
  targetKind: ProjectKind;
  name: string;
  device: string;
  files: ProjectFile[];
  includePaths: string[];
  defines: string[];
  warnings: string[];
}

interface ConversionReport {
  sourceKind: ProjectKind;
  targetKind: ProjectKind;
  device: string;
  outputPath: string;
  generatedFiles: string[];
  warnings: string[];
}

interface BuildValidationReport {
  success: boolean;
  summary: string;
  log: string;
  logPath: string | null;
  validatedProjectPath: string | null;
  cleanupPath: string | null;
}

const app = document.querySelector<HTMLElement>("#app");
if (!app) throw new Error("缺少 #app");

const appIcon = new URL("../src-tauri/icons/icon.ico", import.meta.url).href;

app.innerHTML = `
  <div class="app-shell">
    <header class="topbar">
      <div class="brand">
        <img src="${appIcon}" alt="" />
        <div><strong>TI工具箱</strong><span>MSPM0 开发工具集</span></div>
      </div>
      <div class="local-note"><span></span>所有操作均在本机完成</div>
    </header>

    <nav class="mode-tabs" aria-label="功能切换">
      <button id="mode-convert" data-mode="convert" aria-pressed="true"><strong>工程转换</strong><small>CCS ↔ Keil</small></button>
      <button id="mode-setup" data-mode="setup" aria-pressed="false"><strong>Keil TI 环境配置</strong><small>无需选择工程</small></button>
    </nav>

    <section class="hero">
      <div>
        <p class="eyebrow" id="hero-eyebrow">MSPM0 PROJECT CONVERTER</p>
        <h1 id="hero-title">CCS 与 Keil 工程，双向转换</h1>
        <p class="hero-copy" id="hero-copy">选择开发资源与源工程，工具将基于 TI 官方模板生成目标工程。</p>
      </div>
      <div class="direction-box">
        <small id="hero-side-label">当前方向</small>
        <strong id="direction">等待识别工程</strong>
      </div>
    </section>

    <main class="workflow-card">
      <nav class="progress conversion-only" aria-label="转换步骤">
        <div id="project-progress" data-state="idle"><b>1</b><span><strong>源工程</strong><small>先识别转换方向</small></span></div>
        <i></i>
        <div id="resource-progress" data-state="idle"><b>2</b><span><strong>开发环境</strong><small>按方向自动检测</small></span></div>
        <i></i>
        <div id="output-progress" data-state="idle"><b>3</b><span><strong>输出目录</strong><small>生成目标工程</small></span></div>
      </nav>

      <section class="workflow-section conversion-only" id="project-step" data-state="idle">
        <header class="section-title">
          <div><span>01</span><div><h2>选择源工程</h2><p>解析后只要求当前方向真正需要的资源</p></div></div>
          <div class="section-actions">
            <button class="text-button" id="validate-source" disabled>一键构建验证</button>
            <button class="text-button" id="inspect-project">解析工程</button>
          </div>
        </header>
        <label>
          <span>工程目录</span>
          <div class="path-control"><input id="project-path" readonly placeholder="CCS 目录含 .cproject；Keil 目录含 .uvprojx" /><button id="pick-project">选择工程</button></div>
        </label>
        <div class="build-mode">
          <label>
            <span>CCS 验证方式</span>
            <select id="ccs-build-mode">
              <option value="temporary">临时目录验证（推荐，不修改原工程）</option>
              <option value="in-place">原工程直接构建（更新 Debug/SysConfig）</option>
            </select>
          </label>
          <p><strong class="build-recommendation">建议转换前先执行“一键构建验证”</strong>；没有源 IDE 时仍可带风险继续转换，已有失败结果则必须先修复。</p>
        </div>
        <div class="inspection empty" id="inspection">请先选择工程，工具会自动识别转换方向。</div>
      </section>

      <section class="workflow-section" id="resource-step" data-state="idle">
        <header class="section-title">
          <div><span id="resource-number">02</span><div><h2 id="resource-title">配置开发环境</h2><p id="resource-description">CCS 与 Keil 仅在对应构建验证时需要</p></div></div>
          <div class="section-actions">
            <button class="text-button conversion-only" id="open-keil-setup">独立配置 Keil TI 环境</button>
            <button class="text-button" id="detect-environment" disabled>自动检测环境</button>
          </div>
        </header>
        <div class="field-grid">
          <label>
            <span>MSPM0 SDK 根目录（必需）</span>
            <div class="path-control"><input id="sdk-path" readonly placeholder="包含 .metadata/product.json 的目录" /><button id="pick-sdk">浏览</button></div>
          </label>
          <label>
            <span id="pack-label">CMSIS Pack（CCS → Keil 必需）</span>
            <div class="path-control"><input id="pack-path" readonly placeholder="自动检测已安装 Pack，或选择 .pack/.pdsc" /><button id="pick-pack">浏览</button></div>
            <small class="field-hint">常见位置：&lt;Keil&gt;\ARM\PACK\TexasInstruments 或 &lt;Keil&gt;\ARM\Packs\TexasInstruments；也可直接选择 .pack/.pdsc 文件。</small>
          </label>
          <label class="conversion-only-field">
            <span>CCS 安装目录（可选）</span>
            <div class="path-control"><input id="ccs-path" readonly placeholder="例如 D:\\ti\\ccs2100\\ccs\\theia" /><button id="pick-ccs">浏览</button></div>
          </label>
          <label>
            <span id="keil-label">Keil 安装目录（可选）</span>
            <div class="path-control"><input id="keil-path" readonly placeholder="例如 D:\\Keil_v5" /><button id="pick-keil">浏览</button></div>
          </label>
          <label>
            <span id="sysconfig-label">SysConfig 根目录（可选）</span>
            <div class="path-control"><input id="sysconfig-path" readonly placeholder="需包含 nw/nw.exe 与 sysconfig_cli.bat" /><button id="pick-sysconfig">浏览</button></div>
          </label>
          <label class="search-depth-field">
            <span>工具目录向下搜索层级</span>
            <select id="tool-search-depth">
              <option value="0">0 级（仅当前目录）</option>
              <option value="1">1 级</option>
              <option value="2">2 级（默认）</option>
              <option value="3">3 级</option>
              <option value="4">4 级（最大）</option>
            </select>
          </label>
        </div>
        <div class="inline-result muted resource-result" id="resource-result">
          <span></span><p>解析工程后可自动检测 SDK、Pack、CCS、Keil 与 SysConfig。</p>
          <button class="text-button" id="pack-download" hidden>打开 Pack 下载页</button>
        </div>
      </section>

      <section class="workflow-section conversion-only" id="output-step" data-state="idle">
        <header class="section-title">
          <div><span>03</span><div><h2>设置输出目录</h2><p>为避免误覆盖，只允许使用空目录</p></div></div>
        </header>
        <label>
          <span>目标工程目录</span>
          <div class="path-control"><input id="output-path" readonly placeholder="请选择不存在或完全空白的目录" /><button id="pick-output">选择目录</button></div>
        </label>
      </section>

      <section class="conversion-panel conversion-only">
        <div class="conversion-info">
          <div class="status muted" id="status" role="status" aria-live="polite">
            <strong>准备就绪</strong><span>按顺序完成以上三步即可开始转换。</span>
          </div>
          <div class="safety-note"><span>✓ IDE 可选</span><span>✓ 未验证时明确提示风险</span><span>✓ 不覆盖目标文件</span></div>
        </div>
        <button class="primary" id="convert" disabled><strong>开始转换</strong><small id="convert-caption">请先完成资源、工程和输出配置</small></button>
      </section>

      <section class="setup-panel setup-only">
        <div>
          <div class="status muted" id="setup-status" role="status" aria-live="polite">
            <strong>等待检测 Keil TI 环境</strong><span>自动查找 SDK、Keil 与 SysConfig 后即可一键配置。</span>
          </div>
          <div class="safety-note"><span>✓ 修改前自动备份</span><span>✓ 不覆盖其他 Keil Tools</span><span>✓ Pack 由用户手动安装</span></div>
        </div>
        <button class="primary" id="configure-sysconfig" disabled><strong>一键配置 Keil TI 环境</strong><small>更新 SDK 配置并写入 Keil Tools 菜单</small></button>
      </section>
    </main>

    <footer>TI工具箱 · TI MSPM0 工程转换与环境配置</footer>
  </div>
`;

const sdkInput = element<HTMLInputElement>("sdk-path");
const packInput = element<HTMLInputElement>("pack-path");
const ccsInput = element<HTMLInputElement>("ccs-path");
const keilInput = element<HTMLInputElement>("keil-path");
const sysconfigInput = element<HTMLInputElement>("sysconfig-path");
const projectInput = element<HTMLInputElement>("project-path");
const outputInput = element<HTMLInputElement>("output-path");
const resourceResult = element<HTMLElement>("resource-result");
const inspectionView = element<HTMLElement>("inspection");
const statusView = element<HTMLElement>("status");
const directionView = element<HTMLElement>("direction");
const convertButton = element<HTMLButtonElement>("convert");
const convertCaption = element<HTMLElement>("convert-caption");
const validateSourceButton = element<HTMLButtonElement>("validate-source");
const ccsBuildMode = element<HTMLSelectElement>("ccs-build-mode");
const toolSearchDepth = element<HTMLSelectElement>("tool-search-depth");
const detectEnvironmentButton = element<HTMLButtonElement>("detect-environment");
const configureSysconfigButton = element<HTMLButtonElement>("configure-sysconfig");
const packDownloadButton = element<HTMLButtonElement>("pack-download");
const setupStatusView = element<HTMLElement>("setup-status");
const modeConvertButton = element<HTMLButtonElement>("mode-convert");
const modeSetupButton = element<HTMLButtonElement>("mode-setup");
const heroEyebrow = element<HTMLElement>("hero-eyebrow");
const heroTitle = element<HTMLElement>("hero-title");
const heroCopy = element<HTMLElement>("hero-copy");
const heroSideLabel = element<HTMLElement>("hero-side-label");
const resourceNumber = element<HTMLElement>("resource-number");
const resourceTitle = element<HTMLElement>("resource-title");
const resourceDescription = element<HTMLElement>("resource-description");
const packLabel = element<HTMLElement>("pack-label");
const keilLabel = element<HTMLElement>("keil-label");
const sysconfigLabel = element<HTMLElement>("sysconfig-label");

let environment: EnvironmentDiscovery | null = null;
let setupEnvironment: KeilEnvironmentDiscovery | null = null;
let inspection: ProjectInspection | null = null;
let appMode: AppMode = setting("mode", "convert") === "setup" ? "setup" : "convert";
let sourceValidatedPath = "";
let sourceValidationFailed = false;
let conversionProjectPath = "";
let sourceCleanupPath: string | null = null;
let activeBuildOperation = "";
let liveLogView: HTMLPreElement | null = null;

sdkInput.value = setting("sdkPath");
packInput.value = setting("packPath");
ccsInput.value = setting("ccsPath");
keilInput.value = setting("keilPath");
sysconfigInput.value = setting("sysconfigPath");
toolSearchDepth.value = setting("toolSearchDepth", "2");

void listen<[string, string]>("build-log", ({ payload: [operationId, chunk] }) => {
  if (operationId !== activeBuildOperation || !liveLogView) return;
  liveLogView.textContent += chunk;
  liveLogView.scrollTop = liveLogView.scrollHeight;
});

modeConvertButton.addEventListener("click", () => setMode("convert"));
modeSetupButton.addEventListener("click", () => setMode("setup"));
element("open-keil-setup").addEventListener("click", () => setMode("setup"));

element("pick-sdk").addEventListener("click", async () => {
  const selected = await open({ directory: true, multiple: false, defaultPath: sdkInput.value || undefined });
  if (typeof selected === "string") {
    await discardSourceValidation();
    sdkInput.value = selected;
    saveSetting("sdkPath", selected);
    environment = null;
    setupEnvironment = null;
    if (inspection || appMode === "setup") await detectEnvironment();
  }
});

element("pick-pack").addEventListener("click", async () => {
  const selected = await open({
    multiple: false,
    defaultPath: packInput.value || undefined,
    filters: [{ name: "CMSIS Pack", extensions: ["pack", "pdsc"] }],
  });
  if (typeof selected === "string") {
    await discardSourceValidation();
    packInput.value = selected;
    saveSetting("packPath", selected);
    environment = null;
    setupEnvironment = null;
    if (inspection || appMode === "setup") await detectEnvironment();
  }
});

element("pick-ccs").addEventListener("click", async () => {
  const selected = await open({ directory: true, multiple: false, defaultPath: ccsInput.value || undefined });
  if (typeof selected === "string") {
    await discardSourceValidation();
    ccsInput.value = selected;
    saveSetting("ccsPath", selected);
    environment = null;
    if (inspection) await detectEnvironment();
  }
});

element("pick-keil").addEventListener("click", async () => {
  const selected = await open({ directory: true, multiple: false, defaultPath: keilInput.value || undefined });
  if (typeof selected === "string") {
    await discardSourceValidation();
    keilInput.value = selected;
    saveSetting("keilPath", selected);
    environment = null;
    setupEnvironment = null;
    if (inspection || appMode === "setup") await detectEnvironment();
  }
});

element("pick-sysconfig").addEventListener("click", async () => {
  const selected = await open({ directory: true, multiple: false, defaultPath: sysconfigInput.value || undefined });
  if (typeof selected === "string") {
    sysconfigInput.value = selected;
    saveSetting("sysconfigPath", selected);
    environment = null;
    setupEnvironment = null;
    if (inspection || appMode === "setup") await detectEnvironment();
  }
});

toolSearchDepth.addEventListener("change", async () => {
  await discardSourceValidation();
  saveSetting("toolSearchDepth", toolSearchDepth.value);
  environment = null;
  setupEnvironment = null;
  if (inspection || appMode === "setup") await detectEnvironment();
});

element("pick-project").addEventListener("click", async () => {
  const selected = await open({ directory: true, multiple: false, defaultPath: projectInput.value || undefined });
  if (typeof selected === "string") {
    await discardSourceValidation();
    projectInput.value = selected;
    inspection = null;
    environment = null;
    packDownloadButton.hidden = true;
    markStep("resource", "idle");
    await inspectProject();
  }
});

element("pick-output").addEventListener("click", async () => {
  const selected = await open({ directory: true, multiple: false, defaultPath: outputInput.value || undefined });
  if (typeof selected === "string") {
    outputInput.value = selected;
    markStep("output", "ready");
    showStatus("输出目录已设置", "目标工程将写入所选空目录。");
    updateActionState();
  }
});

element("inspect-project").addEventListener("click", inspectProject);
detectEnvironmentButton.addEventListener("click", detectEnvironment);
configureSysconfigButton.addEventListener("click", configureKeilSysconfig);
packDownloadButton.addEventListener("click", openPackDownload);
validateSourceButton.addEventListener("click", validateSourceProject);
ccsBuildMode.addEventListener("change", () => void discardSourceValidation());
convertButton.addEventListener("click", convertProject);
void setMode(appMode).then(checkForUpdates);

async function checkForUpdates(): Promise<void> {
  let installing = false;
  try {
    const update = await check();
    if (!update || !window.confirm(`发现新版本 ${update.version}，是否立即下载并安装？`)) return;
    installing = true;
    setBusy(true, `正在更新到 ${update.version}`, "下载完成后应用将自动重启…");
    await update.downloadAndInstall();
    await relaunch();
  } catch (error) {
    if (installing) {
      const show = appMode === "setup" ? showSetupStatus : showStatus;
      show("自动更新失败", errorMessage(error), true);
    } else {
      console.warn("检查更新失败", error);
    }
  } finally {
    if (installing) setBusy(false);
  }
}

async function setMode(mode: AppMode): Promise<void> {
  appMode = mode;
  saveSetting("mode", mode);
  document.body.dataset.mode = mode;
  modeConvertButton.setAttribute("aria-pressed", String(mode === "convert"));
  modeSetupButton.setAttribute("aria-pressed", String(mode === "setup"));
  if (mode === "setup") {
    heroEyebrow.textContent = "KEIL TI ENVIRONMENT SETUP";
    heroTitle.textContent = "一键配置 Keil 的 TI 开发环境";
    heroCopy.textContent = "无需选择工程，自动检测 MSPM0 SDK、Keil 与 SysConfig，并安全更新 Keil Tools 菜单。";
    heroSideLabel.textContent = "当前功能";
    directionView.textContent = "Keil TI 环境配置";
    directionView.classList.add("ready");
    resourceNumber.textContent = "01";
    resourceTitle.textContent = "检测并配置环境";
    resourceDescription.textContent = "Pack 手动安装；SDK、Keil 与 SysConfig 可自动检测";
    packLabel.textContent = "CMSIS Pack（手动安装，可选）";
    keilLabel.textContent = "Keil 安装目录（必需）";
    sysconfigLabel.textContent = "SysConfig 根目录（必需）";
    packDownloadButton.hidden = false;
    if (setupEnvironment) renderKeilEnvironment(setupEnvironment);
    else await detectKeilEnvironment();
  } else {
    heroEyebrow.textContent = "MSPM0 PROJECT CONVERTER";
    heroTitle.textContent = "CCS 与 Keil 工程，双向转换";
    heroCopy.textContent = "选择开发资源与源工程，工具将基于 TI 官方模板生成目标工程。";
    heroSideLabel.textContent = "当前方向";
    directionView.textContent = inspection
      ? `${kindLabel(inspection.kind)} → ${kindLabel(inspection.targetKind)}`
      : "等待识别工程";
    directionView.classList.toggle("ready", Boolean(inspection));
    resourceNumber.textContent = "02";
    resourceTitle.textContent = "配置开发环境";
    resourceDescription.textContent = "CCS 与 Keil 仅在对应构建验证时需要";
    packLabel.textContent = "CMSIS Pack（CCS → Keil 必需）";
    keilLabel.textContent = "Keil 安装目录（可选）";
    sysconfigLabel.textContent = "SysConfig 根目录（可选）";
    if (environment) renderEnvironment(environment);
    else {
      resourceResult.className = "inline-result muted resource-result";
      resourceResult.replaceChildren(
        resultDot(),
        textBlock("p", "解析工程后可自动检测 SDK、Pack、CCS、Keil 与 SysConfig。"),
        packDownloadButton,
      );
      packDownloadButton.hidden = true;
    }
  }
  updateActionState();
}

async function detectEnvironment(): Promise<void> {
  if (appMode === "setup") {
    await detectKeilEnvironment();
    return;
  }
  if (!inspection || !projectInput.value) return;
  setBusy(true, "正在自动检测开发环境", "按当前转换方向查找 SDK、Pack、CCS、Keil 与 SysConfig…");
  try {
    environment = await invoke<EnvironmentDiscovery>("discover_environment", {
      request: {
        projectPath: projectInput.value,
        sdkPath: sdkInput.value,
        packPath: packInput.value,
        ccsPath: ccsInput.value,
        keilPath: keilInput.value,
        sysconfigPath: sysconfigInput.value,
        searchDepth: Number(toolSearchDepth.value),
      },
    });
    applyDiscoveredPath(sdkInput, "sdkPath", environment.sdkPath);
    applyDiscoveredPath(packInput, "packPath", environment.packPath);
    applyDiscoveredPath(ccsInput, "ccsPath", environment.ccsPath);
    applyDiscoveredPath(keilInput, "keilPath", environment.keilPath);
    applyDiscoveredPath(sysconfigInput, "sysconfigPath", environment.sysconfigPath);
    renderEnvironment(environment);
    const ready = environmentReady();
    markStep("resource", ready ? "ready" : "error");
    showStatus(
      ready ? "开发环境已就绪" : "开发环境仍缺少必需资源",
      ready ? "可执行构建验证，也可以直接设置输出目录。" : "请根据红色提示补充 SDK 或 Pack。",
      !ready,
    );
  } catch (error) {
    environment = null;
    packDownloadButton.hidden = true;
    resourceResult.className = "inline-result error resource-result";
    resourceResult.replaceChildren(resultDot(), textBlock("p", errorMessage(error)), packDownloadButton);
    markStep("resource", "error");
    showStatus("环境检测失败", errorMessage(error), true);
  } finally {
    setBusy(false);
  }
}

async function detectKeilEnvironment(): Promise<void> {
  environment = null;
  setBusy(true, "正在检测 Keil TI 环境", "查找 MSPM0 SDK、Keil 与 SysConfig…");
  try {
    setupEnvironment = await invoke<KeilEnvironmentDiscovery>("discover_keil_environment", {
      request: {
        sdkPath: sdkInput.value,
        keilPath: keilInput.value,
        sysconfigPath: sysconfigInput.value,
        searchDepth: Number(toolSearchDepth.value),
      },
    });
    applyDiscoveredPath(sdkInput, "sdkPath", setupEnvironment.sdkPath);
    applyDiscoveredPath(keilInput, "keilPath", setupEnvironment.keilPath);
    applyDiscoveredPath(sysconfigInput, "sysconfigPath", setupEnvironment.sysconfigPath);
    renderKeilEnvironment(setupEnvironment);
  } catch (error) {
    setupEnvironment = null;
    resourceResult.className = "inline-result error resource-result";
    resourceResult.replaceChildren(resultDot(), textBlock("p", errorMessage(error)), packDownloadButton);
    packDownloadButton.hidden = false;
    showSetupStatus("环境检测失败", errorMessage(error), true);
  } finally {
    setBusy(false);
  }
}

function renderEnvironment(found: EnvironmentDiscovery): void {
  const pack = inspection?.kind === "ccs"
    ? found.packName
      ? `${found.packName} ${found.packVersion ?? ""} · ${found.packInstalled ? "Keil 已安装" : "未安装，请下载后安装"}`
      : "未找到支持当前芯片的 Pack"
    : "当前方向不需要 Pack";
  const lines = [
    `SDK：${found.sdkVersion ? `${found.sdkVersion} · ${found.sdkPath}` : "未找到（必需）"}`,
    `Pack：${pack}`,
    `CCS：${found.ccsExecutable ?? "未找到（仅影响 CCS 构建验证）"}`,
    `Keil：${found.keilExecutable ?? "未找到（仅影响 Keil 构建验证）"}`,
    `SysConfig：${found.sysconfigExecutable ?? "未找到（已有生成文件仍可转换）"}`,
    ...found.warnings.map((warning) => `提示：${warning}`),
  ];
  const ready = environmentReady(found);
  resourceResult.className = `inline-result ${ready ? "success" : "error"} resource-result`;
  packDownloadButton.hidden = found.packInstalled || !found.packDownloadUrl;
  resourceResult.replaceChildren(resultDot(), textBlock("p", lines.join("\n")), packDownloadButton);
}

function renderKeilEnvironment(found: KeilEnvironmentDiscovery): void {
  const ready = keilSetupReady(found);
  const lines = [
    `SDK：${found.sdkVersion ? `${found.sdkVersion} · ${found.sdkPath}` : "未找到（必需）"}`,
    `Keil：${found.keilExecutable ?? "未找到（必需）"}`,
    `SysConfig：${found.sysconfigExecutable ?? "未找到（必需）"}`,
    `Pack：${packInput.value || "请手动安装到 Keil ARM\\PACK 或 ARM\\Packs；不影响 SysConfig 配置"}`,
    ...found.warnings.map((warning) => `提示：${warning}`),
  ];
  resourceResult.className = `inline-result ${ready ? "success" : "error"} resource-result`;
  resourceResult.replaceChildren(resultDot(), textBlock("p", lines.join("\n")), packDownloadButton);
  packDownloadButton.hidden = false;
  markStep("resource", ready ? "ready" : "error");
  showSetupStatus(
    ready ? "Keil TI 环境已就绪" : "仍缺少一键配置所需路径",
    ready ? "可以执行一键配置；操作前会再次确认。" : "请根据红色提示补充 SDK、Keil 或 SysConfig。",
    !ready,
  );
}

async function configureKeilSysconfig(): Promise<void> {
  if (!setupEnvironment?.sdkPath || !setupEnvironment.keilPath || !setupEnvironment.sysconfigPath) return;
  const message = `将备份并更新以下 SDK 配置：\n${setupEnvironment.sdkPath}\\tools\\keil\\syscfg.bat\n${setupEnvironment.sdkPath}\\tools\\keil\\MSPM0_SDK_syscfg_menu_import.cfg\n\n同时写入当前用户的 Keil Tools 菜单。是否继续？`;
  if (!window.confirm(message)) return;
  setBusy(true, "正在配置 Keil SysConfig", "备份 SDK 配置并更新 Keil Tools 菜单…");
  try {
    const result = await invoke<KeilSysConfigResult>("configure_keil_sysconfig", {
      request: {
        sdkPath: setupEnvironment.sdkPath,
        keilPath: setupEnvironment.keilPath,
        sysconfigPath: setupEnvironment.sysconfigPath,
        searchDepth: Number(toolSearchDepth.value),
      },
    });
    showSetupStatus(
      result.changed ? "Keil SysConfig 配置完成" : "Keil SysConfig 已经配置完成",
      `Tools 槽位 ${result.slot} · ${result.title}${result.updatedFiles.length ? ` · 已更新 ${result.updatedFiles.length} 个 SDK 文件` : ""}${result.backupFiles.length ? ` · 已创建 ${result.backupFiles.length} 个备份` : ""}`,
    );
  } catch (error) {
    showSetupStatus("Keil SysConfig 配置失败", errorMessage(error), true);
  } finally {
    setBusy(false);
  }
}

async function openPackDownload(): Promise<void> {
  const url = appMode === "setup"
    ? "https://www.keil.arm.com/packs/?q=MSPM0"
    : environment?.packDownloadUrl;
  if (!url) return;
  try {
    await invoke("open_pack_download", { url });
  } catch (error) {
    showStatus("无法打开 Pack 下载页", errorMessage(error), true);
  }
}

function applyDiscoveredPath(input: HTMLInputElement, storageKey: string, value: string | null): void {
  if (!value) return;
  input.value = value;
  saveSetting(storageKey, value);
}

async function inspectProject(): Promise<void> {
  if (!projectInput.value) {
    showStatus("尚未选择工程", "请选择 CCS 或 Keil 工程目录。", true);
    return;
  }
  await discardSourceValidation();
  environment = null;
  markStep("resource", "idle");
  setBusy(true, "正在解析源工程", "读取工程配置和文件清单…");
  let parsed = false;
  try {
    inspection = await invoke<ProjectInspection>("inspect_project", { projectPath: projectInput.value });
    renderInspection(inspection);
    directionView.textContent = `${kindLabel(inspection.kind)} → ${kindLabel(inspection.targetKind)}`;
    directionView.classList.add("ready");
    markStep("project", "ready");
    showStatus(`已解析 ${inspection.name}`, `${inspection.device} · ${inspection.files.length} 个工程文件；正在检测所需环境`);
    parsed = true;
  } catch (error) {
    inspection = null;
    inspectionView.className = "inspection error";
    inspectionView.textContent = errorMessage(error);
    directionView.textContent = "工程解析失败";
    directionView.classList.remove("ready");
    markStep("project", "error");
    showStatus("工程解析失败", errorMessage(error), true);
  } finally {
    setBusy(false);
  }
  if (parsed) await detectEnvironment();
}

async function validateSourceProject(): Promise<void> {
  if (!inspection || !sourceToolAvailable()) return;
  const ccsInPlace = ccsBuildMode.value === "in-place";
  if (inspection.kind === "ccs" && ccsInPlace && !window.confirm("原工程直接构建会执行 CCS Clean + Full Build，并更新源工程的 Debug、SysConfig 等构建产物。是否继续？")) return;
  setBusy(true, `正在执行 ${kindLabel(inspection.kind)} 构建验证`, inspection.kind === "ccs" ? `${ccsInPlace ? "在原工程" : "在临时副本"}执行 Clean + Full Build，再关闭未使用 section 消除进行严格链接…` : "调用 Keil 构建源工程…");
  try {
    const report = await runBuildValidation(projectInput.value, ccsInPlace);
    renderBuildReport(inspectionView, report);
    sourceValidatedPath = report.success ? projectInput.value : "";
    sourceValidationFailed = !report.success;
    conversionProjectPath = report.validatedProjectPath ?? projectInput.value;
    sourceCleanupPath = report.cleanupPath;
    markStep("project", report.success ? "ready" : "error");
    showStatus(report.summary, report.success ? "源工程验证通过，可以开始转换。" : "源工程本身未通过严格验证，请先修复日志中的问题。", !report.success);
  } catch (error) {
    sourceValidatedPath = "";
    sourceValidationFailed = false;
    conversionProjectPath = "";
    sourceCleanupPath = null;
    markStep("project", "error");
    showStatus("构建验证无法完成", errorMessage(error), true);
  } finally {
    setBusy(false);
  }
}

async function convertProject(): Promise<void> {
  if (!environmentReady() || !inspection || sourceValidationFailed || !outputInput.value) return;
  if (sourceValidatedPath !== projectInput.value && !window.confirm("源工程尚未通过真实工具链构建验证，转换结果可能保留原工程中的编译或链接错误。是否仍要继续转换？")) return;
  setBusy(true, `正在生成 ${kindLabel(inspection.targetKind)} 工程`, "复制源码并生成目标工程配置…");
  try {
    const report = await invoke<ConversionReport>("convert_project", {
      request: {
        projectPath: conversionProjectPath || projectInput.value,
        sdkPath: sdkInput.value,
        packPath: packInput.value,
        outputPath: outputInput.value,
      },
    });
    const canValidateTarget = report.targetKind === "ccs" ? Boolean(environment?.ccsExecutable) : Boolean(environment?.keilExecutable);
    if (!canValidateTarget) {
      statusView.className = "status success report";
      statusView.replaceChildren(
        textBlock("strong", `转换完成，未执行 ${kindLabel(report.targetKind)} 构建验证`),
        textBlock("span", `${report.device} · 共生成 ${report.generatedFiles.length} 个文件；未配置目标 IDE，可稍后手动构建`),
        textBlock("code", report.outputPath),
        ...(report.warnings.length ? [textBlock("small", report.warnings.join("；"))] : []),
      );
      markStep("output", "complete");
      await discardSourceValidation();
      return;
    }
    showStatus(`转换完成，正在验证 ${kindLabel(report.targetKind)} 工程`, "调用目标工具链进行真实构建…");
    let validation: BuildValidationReport;
    try {
      validation = await runBuildValidation(report.outputPath, false);
    } catch (error) {
      statusView.className = "status error report";
      statusView.replaceChildren(
        textBlock("strong", `转换完成，但无法启动 ${kindLabel(report.targetKind)} 构建验证`),
        textBlock("span", errorMessage(error)),
        textBlock("code", report.outputPath),
      );
      markStep("output", "error");
      await discardSourceValidation();
      return;
    }
    statusView.className = `status ${validation.success ? "success" : "error"} report`;
    statusView.replaceChildren(
      textBlock("strong", validation.success ? `转换与 ${kindLabel(report.targetKind)} 构建验证均通过` : `转换完成，但 ${kindLabel(report.targetKind)} 构建验证失败`),
      textBlock("span", `${report.device} · 共生成 ${report.generatedFiles.length} 个文件 · ${validation.summary}`),
      textBlock("code", report.outputPath),
      ...(report.warnings.length ? [textBlock("small", report.warnings.join("；"))] : []),
    );
    renderBuildReport(statusView, validation);
    markStep("output", validation.success ? "complete" : "error");
    if (validation.cleanupPath) await cleanupValidationCopy(validation.cleanupPath);
    await discardSourceValidation();
  } catch (error) {
    showStatus("转换失败", errorMessage(error), true);
  } finally {
    setBusy(false);
  }
}

function renderInspection(result: ProjectInspection): void {
  ccsBuildMode.disabled = result.kind !== "ccs";
  inspectionView.className = "inspection";
  const details = document.createElement("div");
  details.className = "summary-grid";
  details.append(
    metric("工程名称", result.name),
    metric("目标芯片", result.device),
    metric("转换方向", `${kindLabel(result.kind)} → ${kindLabel(result.targetKind)}`),
    metric("工程文件", String(result.files.length)),
  );
  const preview = result.files.slice(0, 6).map((file) => file.path).join("  ·  ");
  const files = textBlock("p", preview || "没有识别到源文件");
  files.className = "file-preview";
  inspectionView.replaceChildren(details, files);
  if (result.warnings.length) {
    const warnings = textBlock("p", result.warnings.join("；"));
    warnings.className = "warning";
    inspectionView.append(warnings);
  }
}

function renderBuildReport(container: HTMLElement, report: BuildValidationReport): void {
  container.querySelector(".validation-result")?.remove();
  const result = document.createElement("div");
  result.className = `validation-result ${report.success ? "success" : "error"}`;
  result.append(textBlock("strong", report.summary));
  if (report.logPath) result.append(textBlock("code", report.logPath));
  if (!report.success) {
    const excerpt = report.log.trimEnd().split(/\r?\n/).slice(-18).join("\n");
    const errorLog = document.createElement("pre");
    errorLog.className = "error-excerpt";
    errorLog.textContent = excerpt || "构建工具未返回日志。";
    result.append(textBlock("small", "错误附近日志（末尾 18 行）"), errorLog);
  }
  const details = document.createElement("details");
  const summary = document.createElement("summary");
  summary.textContent = "查看完整构建日志";
  const log = document.createElement("pre");
  log.textContent = report.log;
  details.append(summary, log);
  result.append(details);
  container.append(result);
}

async function runBuildValidation(projectPath: string, ccsInPlace: boolean): Promise<BuildValidationReport> {
  const operationId = crypto.randomUUID();
  activeBuildOperation = operationId;
  const panel = document.createElement("div");
  panel.className = "validation-result live-log";
  panel.append(textBlock("strong", "实时构建日志"));
  liveLogView = document.createElement("pre");
  panel.append(liveLogView);
  statusView.append(panel);
  try {
    return await invoke<BuildValidationReport>("validate_project_build", {
      projectPath,
      ccsPath: ccsInput.value,
      keilPath: keilInput.value,
      ccsInPlace,
      searchDepth: Number(toolSearchDepth.value),
      operationId,
    });
  } finally {
    activeBuildOperation = "";
    liveLogView = null;
  }
}

function metric(label: string, value: string): HTMLElement {
  const item = document.createElement("div");
  item.append(textBlock("span", label), textBlock("strong", value));
  return item;
}

function resultDot(): HTMLElement {
  return document.createElement("span");
}

function textBlock(tag: "span" | "strong" | "code" | "small" | "p", text: string): HTMLElement {
  const node = document.createElement(tag);
  node.textContent = text;
  return node;
}

function element<T extends HTMLElement = HTMLElement>(id: string): T {
  const node = document.getElementById(id);
  if (!node) throw new Error(`缺少 #${id}`);
  return node as T;
}

function kindLabel(kind: ProjectKind): string {
  return kind === "ccs" ? "CCS" : "Keil";
}

function markStep(name: "resource" | "project" | "output", state: "idle" | "ready" | "error" | "complete"): void {
  element(`${name}-step`).dataset.state = state;
  element(`${name}-progress`).dataset.state = state;
}

function setBusy(busy: boolean, title?: string, detail?: string): void {
  document.body.classList.toggle("is-busy", busy);
  document.querySelectorAll<HTMLButtonElement>("button").forEach((button) => {
    button.disabled = busy;
  });
  if (title) {
    if (appMode === "setup") showSetupStatus(title, detail ?? "");
    else showStatus(title, detail ?? "");
  }
  if (!busy) {
    document.querySelectorAll<HTMLButtonElement>("button").forEach((button) => {
      button.disabled = false;
    });
    updateActionState();
  }
}

function updateActionState(): void {
  detectEnvironmentButton.disabled = appMode === "convert" && !inspection;
  validateSourceButton.disabled = !sourceToolAvailable();
  validateSourceButton.title = inspection && !sourceToolAvailable()
    ? `未配置 ${kindLabel(inspection.kind)}，无法执行源工程构建验证`
    : "";
  configureSysconfigButton.disabled = appMode !== "setup" || !keilSetupReady();
  configureSysconfigButton.title = configureSysconfigButton.disabled
    ? "需要 SDK、Keil 与 SysConfig 路径"
    : "备份并更新 SDK 配置文件和 Keil Tools 菜单";
  const missing = [
    !inspection && "源工程",
    inspection && !environment?.sdkPath && "MSPM0 SDK",
    inspection?.kind === "ccs" && !environment?.packPath && "CMSIS Pack",
    sourceValidationFailed && "修复构建错误",
    !outputInput.value && "输出目录",
  ].filter(Boolean);
  convertButton.disabled = missing.length > 0;
  convertCaption.textContent = missing.length
    ? `还需设置：${missing.join("、")}`
    : sourceValidatedPath === projectInput.value
      ? "源工程验证通过，可以开始转换"
      : "可以转换，建议先执行构建验证";
}

function keilSetupReady(found = setupEnvironment): boolean {
  return Boolean(found?.sdkPath && found.keilPath && found.sysconfigPath);
}

function environmentReady(found = environment): boolean {
  return Boolean(found?.sdkPath && (found.projectKind !== "ccs" || found.packPath));
}

function sourceToolAvailable(): boolean {
  if (!inspection || !environment) return false;
  return inspection.kind === "ccs" ? Boolean(environment.ccsExecutable) : Boolean(environment.keilExecutable);
}

async function discardSourceValidation(): Promise<void> {
  if (sourceCleanupPath) await cleanupValidationCopy(sourceCleanupPath);
  sourceValidatedPath = "";
  sourceValidationFailed = false;
  conversionProjectPath = "";
  sourceCleanupPath = null;
  updateActionState();
}

async function cleanupValidationCopy(path: string): Promise<void> {
  try {
    await invoke("cleanup_validation_copy", { path });
  } catch (error) {
    console.warn("清理 CCS 临时验证目录失败", error);
  }
}

function showStatus(title: string, detail = "", error = false): void {
  statusView.className = error ? "status error" : "status muted";
  statusView.replaceChildren(textBlock("strong", title), textBlock("span", detail));
}

function showSetupStatus(title: string, detail = "", error = false): void {
  setupStatusView.className = error ? "status error" : "status muted";
  setupStatusView.replaceChildren(textBlock("strong", title), textBlock("span", detail));
}

function setting(key: string, fallback = ""): string {
  return localStorage.getItem(`ti-toolbox.${key}`) ?? fallback;
}

function saveSetting(key: string, value: string): void {
  localStorage.setItem(`ti-toolbox.${key}`, value);
}

function errorMessage(error: unknown): string {
  return typeof error === "string" ? error : error instanceof Error ? error.message : JSON.stringify(error);
}
