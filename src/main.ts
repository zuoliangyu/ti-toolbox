import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import "./styles.css";

type ProjectKind = "ccs" | "keil";

interface ResourceInfo {
  sdkVersion: string;
  packName: string;
  packVersion: string;
  devices: string[];
  ccsExecutable: string;
  keilExecutable: string;
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
        <div><strong>CCS2KEIL</strong><span>MSPM0 工程转换器</span></div>
      </div>
      <div class="local-note"><span></span>所有操作均在本机完成</div>
    </header>

    <section class="hero">
      <div>
        <p class="eyebrow">MSPM0 PROJECT CONVERTER</p>
        <h1>CCS 与 Keil 工程，双向转换</h1>
        <p class="hero-copy">选择开发资源与源工程，工具将基于 TI 官方模板生成目标工程。</p>
      </div>
      <div class="direction-box">
        <small>当前方向</small>
        <strong id="direction">等待识别工程</strong>
      </div>
    </section>

    <main class="workflow-card">
      <nav class="progress" aria-label="转换步骤">
        <div id="resource-progress" data-state="idle"><b>1</b><span><strong>开发资源</strong><small>SDK、Pack 与 IDE</small></span></div>
        <i></i>
        <div id="project-progress" data-state="idle"><b>2</b><span><strong>源工程</strong><small>自动识别类型</small></span></div>
        <i></i>
        <div id="output-progress" data-state="idle"><b>3</b><span><strong>输出目录</strong><small>生成目标工程</small></span></div>
      </nav>

      <section class="workflow-section" id="resource-step" data-state="idle">
        <header class="section-title">
          <div><span>01</span><div><h2>配置开发资源</h2><p>SDK、Pack、CCS 与 Keil 路径只保存在当前电脑</p></div></div>
          <button class="text-button" id="validate-resources">验证资源</button>
        </header>
        <div class="field-grid">
          <label>
            <span>MSPM0 SDK 根目录</span>
            <div class="path-control"><input id="sdk-path" readonly placeholder="包含 .metadata/product.json 的目录" /><button id="pick-sdk">浏览</button></div>
          </label>
          <label>
            <span>CMSIS Pack 文件</span>
            <div class="path-control"><input id="pack-path" readonly placeholder="TexasInstruments.*.pack" /><button id="pick-pack">浏览</button></div>
          </label>
          <label>
            <span>CCS 安装目录</span>
            <div class="path-control"><input id="ccs-path" readonly placeholder="例如 D:\\ti\\ccs2100\\ccs\\theia" /><button id="pick-ccs">浏览</button></div>
          </label>
          <label>
            <span>Keil 安装目录</span>
            <div class="path-control"><input id="keil-path" readonly placeholder="例如 D:\\Keil_v5" /><button id="pick-keil">浏览</button></div>
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
        <div class="inline-result muted" id="resource-result"><span></span><p>选择资源后会自动验证版本和器件支持。</p></div>
      </section>

      <section class="workflow-section" id="project-step" data-state="idle">
        <header class="section-title">
          <div><span>02</span><div><h2>选择源工程</h2><p>解析配置后执行真实工具链构建验证</p></div></div>
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
          <p>无论选择哪种方式，都会在普通构建后执行一次保留死代码的严格链接。</p>
        </div>
        <div class="inspection empty" id="inspection">工程解析与构建验证结果会显示在这里。</div>
      </section>

      <section class="workflow-section" id="output-step" data-state="idle">
        <header class="section-title">
          <div><span>03</span><div><h2>设置输出目录</h2><p>为避免误覆盖，只允许使用空目录</p></div></div>
        </header>
        <label>
          <span>目标工程目录</span>
          <div class="path-control"><input id="output-path" readonly placeholder="请选择不存在或完全空白的目录" /><button id="pick-output">选择目录</button></div>
        </label>
      </section>

      <section class="conversion-panel">
        <div class="conversion-info">
          <div class="status muted" id="status" role="status" aria-live="polite">
            <strong>准备就绪</strong><span>按顺序完成以上三步即可开始转换。</span>
          </div>
          <div class="safety-note"><span>✓ 转换过程只读</span><span>✓ CCS 验证会更新构建产物</span><span>✓ 不覆盖目标文件</span></div>
        </div>
        <button class="primary" id="convert" disabled><strong>开始转换</strong><small id="convert-caption">请先完成资源、工程和输出配置</small></button>
      </section>
    </main>

    <footer>CCS2KEIL · TI MSPM0 NoRTOS DriverLib Project Bridge</footer>
  </div>
`;

const sdkInput = element<HTMLInputElement>("sdk-path");
const packInput = element<HTMLInputElement>("pack-path");
const ccsInput = element<HTMLInputElement>("ccs-path");
const keilInput = element<HTMLInputElement>("keil-path");
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

let resources: ResourceInfo | null = null;
let inspection: ProjectInspection | null = null;
let sourceValidatedPath = "";
let conversionProjectPath = "";
let sourceCleanupPath: string | null = null;
let activeBuildOperation = "";
let liveLogView: HTMLPreElement | null = null;

sdkInput.value = localStorage.getItem("ccs2keil.sdkPath") ?? "";
packInput.value = localStorage.getItem("ccs2keil.packPath") ?? "";
ccsInput.value = localStorage.getItem("ccs2keil.ccsPath") ?? "";
keilInput.value = localStorage.getItem("ccs2keil.keilPath") ?? "";
toolSearchDepth.value = localStorage.getItem("ccs2keil.toolSearchDepth") ?? "2";

void listen<[string, string]>("build-log", ({ payload: [operationId, chunk] }) => {
  if (operationId !== activeBuildOperation || !liveLogView) return;
  liveLogView.textContent += chunk;
  liveLogView.scrollTop = liveLogView.scrollHeight;
});

element("pick-sdk").addEventListener("click", async () => {
  const selected = await open({ directory: true, multiple: false, defaultPath: sdkInput.value || undefined });
  if (typeof selected === "string") {
    await discardSourceValidation();
    sdkInput.value = selected;
    localStorage.setItem("ccs2keil.sdkPath", selected);
    resources = null;
    await validateResources();
  }
});

element("pick-pack").addEventListener("click", async () => {
  const selected = await open({
    multiple: false,
    defaultPath: packInput.value || undefined,
    filters: [{ name: "CMSIS Pack", extensions: ["pack"] }],
  });
  if (typeof selected === "string") {
    await discardSourceValidation();
    packInput.value = selected;
    localStorage.setItem("ccs2keil.packPath", selected);
    resources = null;
    await validateResources();
  }
});

element("pick-ccs").addEventListener("click", async () => {
  const selected = await open({ directory: true, multiple: false, defaultPath: ccsInput.value || undefined });
  if (typeof selected === "string") {
    await discardSourceValidation();
    ccsInput.value = selected;
    localStorage.setItem("ccs2keil.ccsPath", selected);
    resources = null;
    await validateResources();
  }
});

element("pick-keil").addEventListener("click", async () => {
  const selected = await open({ directory: true, multiple: false, defaultPath: keilInput.value || undefined });
  if (typeof selected === "string") {
    await discardSourceValidation();
    keilInput.value = selected;
    localStorage.setItem("ccs2keil.keilPath", selected);
    resources = null;
    await validateResources();
  }
});

toolSearchDepth.addEventListener("change", async () => {
  await discardSourceValidation();
  localStorage.setItem("ccs2keil.toolSearchDepth", toolSearchDepth.value);
  resources = null;
  await validateResources();
});

element("pick-project").addEventListener("click", async () => {
  const selected = await open({ directory: true, multiple: false, defaultPath: projectInput.value || undefined });
  if (typeof selected === "string") {
    await discardSourceValidation();
    projectInput.value = selected;
    inspection = null;
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

element("validate-resources").addEventListener("click", validateResources);
element("inspect-project").addEventListener("click", inspectProject);
validateSourceButton.addEventListener("click", validateSourceProject);
ccsBuildMode.addEventListener("change", () => void discardSourceValidation());
convertButton.addEventListener("click", convertProject);

if (sdkInput.value && packInput.value && ccsInput.value && keilInput.value) void validateResources();

async function validateResources(): Promise<void> {
  if (!sdkInput.value || !packInput.value || !ccsInput.value || !keilInput.value) {
    showStatus("资源尚未配置", "请先选择 SDK、Pack、CCS 和 Keil。", true);
    return;
  }
  setBusy(true, "正在验证开发资源", "读取 SDK 与 Pack 元数据…");
  try {
    resources = await invoke<ResourceInfo>("validate_resources", {
      sdkPath: sdkInput.value,
      packPath: packInput.value,
      ccsPath: ccsInput.value,
      keilPath: keilInput.value,
      searchDepth: Number(toolSearchDepth.value),
    });
    resourceResult.className = "inline-result success";
    resourceResult.replaceChildren(resultDot(), textBlock("p", `SDK ${resources.sdkVersion} · ${resources.packName} ${resources.packVersion} · 支持 ${resources.devices.length} 个器件\nCCS ${resources.ccsExecutable}\nKeil ${resources.keilExecutable}`));
    markStep("resource", "ready");
    showStatus("开发资源验证通过", "现在可以选择需要转换的工程。");
  } catch (error) {
    resources = null;
    resourceResult.className = "inline-result error";
    resourceResult.replaceChildren(resultDot(), textBlock("p", errorMessage(error)));
    markStep("resource", "error");
    showStatus("资源验证失败", errorMessage(error), true);
  } finally {
    setBusy(false);
  }
}

async function inspectProject(): Promise<void> {
  if (!projectInput.value) {
    showStatus("尚未选择工程", "请选择 CCS 或 Keil 工程目录。", true);
    return;
  }
  await discardSourceValidation();
  setBusy(true, "正在解析源工程", "读取工程配置和文件清单…");
  try {
    inspection = await invoke<ProjectInspection>("inspect_project", { projectPath: projectInput.value });
    renderInspection(inspection);
    directionView.textContent = `${kindLabel(inspection.kind)} → ${kindLabel(inspection.targetKind)}`;
    directionView.classList.add("ready");
    markStep("project", "ready");
    showStatus(`已解析 ${inspection.name}`, `${inspection.device} · ${inspection.files.length} 个工程文件；请继续执行一键构建验证`);
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
}

async function validateSourceProject(): Promise<void> {
  if (!inspection || !resources) return;
  const ccsInPlace = ccsBuildMode.value === "in-place";
  if (inspection.kind === "ccs" && ccsInPlace && !window.confirm("原工程直接构建会执行 CCS Clean + Full Build，并更新源工程的 Debug、SysConfig 等构建产物。是否继续？")) return;
  setBusy(true, `正在执行 ${kindLabel(inspection.kind)} 构建验证`, inspection.kind === "ccs" ? `${ccsInPlace ? "在原工程" : "在临时副本"}执行 Clean + Full Build，再关闭未使用 section 消除进行严格链接…` : "调用 Keil 构建源工程…");
  try {
    const report = await runBuildValidation(projectInput.value, ccsInPlace);
    renderBuildReport(inspectionView, report);
    sourceValidatedPath = report.success ? projectInput.value : "";
    conversionProjectPath = report.validatedProjectPath ?? projectInput.value;
    sourceCleanupPath = report.cleanupPath;
    markStep("project", report.success ? "ready" : "error");
    showStatus(report.summary, report.success ? "源工程验证通过，可以开始转换。" : "源工程本身未通过严格验证，请先修复日志中的问题。", !report.success);
  } catch (error) {
    sourceValidatedPath = "";
    conversionProjectPath = "";
    sourceCleanupPath = null;
    markStep("project", "error");
    showStatus("构建验证无法完成", errorMessage(error), true);
  } finally {
    setBusy(false);
  }
}

async function convertProject(): Promise<void> {
  if (!resources || !inspection || sourceValidatedPath !== projectInput.value || !outputInput.value) return;
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
  const details = document.createElement("details");
  const summary = document.createElement("summary");
  summary.textContent = "查看构建日志";
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
  if (title) showStatus(title, detail ?? "");
  if (!busy) {
    document.querySelectorAll<HTMLButtonElement>("button").forEach((button) => {
      button.disabled = false;
    });
    updateActionState();
  }
}

function updateActionState(): void {
  validateSourceButton.disabled = !resources || !inspection;
  const missing = [
    !resources && "开发资源",
    !inspection && "源工程",
    inspection && sourceValidatedPath !== projectInput.value && "构建验证",
    !outputInput.value && "输出目录",
  ].filter(Boolean);
  convertButton.disabled = missing.length > 0;
  convertCaption.textContent = missing.length ? `还需设置：${missing.join("、")}` : "配置完整，可以开始生成目标工程";
}

async function discardSourceValidation(): Promise<void> {
  if (sourceCleanupPath) await cleanupValidationCopy(sourceCleanupPath);
  sourceValidatedPath = "";
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

function errorMessage(error: unknown): string {
  return typeof error === "string" ? error : error instanceof Error ? error.message : JSON.stringify(error);
}
