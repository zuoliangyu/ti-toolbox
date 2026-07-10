import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import "./styles.css";

type ProjectKind = "ccs" | "keil";

interface ResourceInfo {
  sdkVersion: string;
  packName: string;
  packVersion: string;
  devices: string[];
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
        <div id="resource-progress" data-state="idle"><b>1</b><span><strong>开发资源</strong><small>SDK 与 Pack</small></span></div>
        <i></i>
        <div id="project-progress" data-state="idle"><b>2</b><span><strong>源工程</strong><small>自动识别类型</small></span></div>
        <i></i>
        <div id="output-progress" data-state="idle"><b>3</b><span><strong>输出目录</strong><small>生成目标工程</small></span></div>
      </nav>

      <section class="workflow-section" id="resource-step" data-state="idle">
        <header class="section-title">
          <div><span>01</span><div><h2>配置开发资源</h2><p>SDK 与 Pack 路径只保存在当前电脑</p></div></div>
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
        </div>
        <div class="inline-result muted" id="resource-result"><span></span><p>选择资源后会自动验证版本和器件支持。</p></div>
      </section>

      <section class="workflow-section" id="project-step" data-state="idle">
        <header class="section-title">
          <div><span>02</span><div><h2>选择源工程</h2><p>支持 CCS 与 Keil 工程目录</p></div></div>
          <button class="text-button" id="inspect-project">检查工程</button>
        </header>
        <label>
          <span>工程目录</span>
          <div class="path-control"><input id="project-path" readonly placeholder="CCS 目录含 .cproject；Keil 目录含 .uvprojx" /><button id="pick-project">选择工程</button></div>
        </label>
        <div class="inspection empty" id="inspection">工程识别结果会显示在这里。</div>
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
          <div class="safety-note"><span>✓ 源工程只读</span><span>✓ 不覆盖已有文件</span><span>✓ 失败自动清理</span></div>
        </div>
        <button class="primary" id="convert" disabled><strong>开始转换</strong><small id="convert-caption">请先完成资源、工程和输出配置</small></button>
      </section>
    </main>

    <footer>CCS2KEIL · TI MSPM0 NoRTOS DriverLib Project Bridge</footer>
  </div>
`;

const sdkInput = element<HTMLInputElement>("sdk-path");
const packInput = element<HTMLInputElement>("pack-path");
const projectInput = element<HTMLInputElement>("project-path");
const outputInput = element<HTMLInputElement>("output-path");
const resourceResult = element<HTMLElement>("resource-result");
const inspectionView = element<HTMLElement>("inspection");
const statusView = element<HTMLElement>("status");
const directionView = element<HTMLElement>("direction");
const convertButton = element<HTMLButtonElement>("convert");
const convertCaption = element<HTMLElement>("convert-caption");

let resources: ResourceInfo | null = null;
let inspection: ProjectInspection | null = null;

sdkInput.value = localStorage.getItem("ccs2keil.sdkPath") ?? "";
packInput.value = localStorage.getItem("ccs2keil.packPath") ?? "";

element("pick-sdk").addEventListener("click", async () => {
  const selected = await open({ directory: true, multiple: false, defaultPath: sdkInput.value || undefined });
  if (typeof selected === "string") {
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
    packInput.value = selected;
    localStorage.setItem("ccs2keil.packPath", selected);
    resources = null;
    await validateResources();
  }
});

element("pick-project").addEventListener("click", async () => {
  const selected = await open({ directory: true, multiple: false, defaultPath: projectInput.value || undefined });
  if (typeof selected === "string") {
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
    updateConvertState();
  }
});

element("validate-resources").addEventListener("click", validateResources);
element("inspect-project").addEventListener("click", inspectProject);
convertButton.addEventListener("click", convertProject);

if (sdkInput.value && packInput.value) void validateResources();

async function validateResources(): Promise<void> {
  if (!sdkInput.value || !packInput.value) {
    showStatus("资源尚未配置", "请先选择 SDK 和 Pack。", true);
    return;
  }
  setBusy(true, "正在验证开发资源", "读取 SDK 与 Pack 元数据…");
  try {
    resources = await invoke<ResourceInfo>("validate_resources", {
      sdkPath: sdkInput.value,
      packPath: packInput.value,
    });
    resourceResult.className = "inline-result success";
    resourceResult.replaceChildren(resultDot(), textBlock("p", `SDK ${resources.sdkVersion} · ${resources.packName} ${resources.packVersion} · 支持 ${resources.devices.length} 个器件`));
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
  setBusy(true, "正在检查源工程", "读取工程配置和文件清单…");
  try {
    inspection = await invoke<ProjectInspection>("inspect_project", { projectPath: projectInput.value });
    renderInspection(inspection);
    directionView.textContent = `${kindLabel(inspection.kind)} → ${kindLabel(inspection.targetKind)}`;
    directionView.classList.add("ready");
    markStep("project", "ready");
    showStatus(`已识别 ${inspection.name}`, `${inspection.device} · ${inspection.files.length} 个工程文件`);
  } catch (error) {
    inspection = null;
    inspectionView.className = "inspection error";
    inspectionView.textContent = errorMessage(error);
    directionView.textContent = "工程识别失败";
    directionView.classList.remove("ready");
    markStep("project", "error");
    showStatus("工程检查失败", errorMessage(error), true);
  } finally {
    setBusy(false);
  }
}

async function convertProject(): Promise<void> {
  if (!resources || !inspection || !outputInput.value) return;
  setBusy(true, `正在生成 ${kindLabel(inspection.targetKind)} 工程`, "复制源码并生成目标工程配置…");
  try {
    const report = await invoke<ConversionReport>("convert_project", {
      request: {
        projectPath: projectInput.value,
        sdkPath: sdkInput.value,
        packPath: packInput.value,
        outputPath: outputInput.value,
      },
    });
    statusView.className = "status success report";
    statusView.replaceChildren(
      textBlock("strong", `转换完成 · ${kindLabel(report.sourceKind)} → ${kindLabel(report.targetKind)}`),
      textBlock("span", `${report.device} · 共生成 ${report.generatedFiles.length} 个文件`),
      textBlock("code", report.outputPath),
      ...(report.warnings.length ? [textBlock("small", report.warnings.join("；"))] : []),
    );
    markStep("output", "complete");
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
  if (!busy) updateConvertState();
}

function updateConvertState(): void {
  const missing = [!resources && "开发资源", !inspection && "源工程", !outputInput.value && "输出目录"].filter(Boolean);
  convertButton.disabled = missing.length > 0;
  convertCaption.textContent = missing.length ? `还需设置：${missing.join("、")}` : "配置完整，可以开始生成目标工程";
}

function showStatus(title: string, detail = "", error = false): void {
  statusView.className = error ? "status error" : "status muted";
  statusView.replaceChildren(textBlock("strong", title), textBlock("span", detail));
}

function errorMessage(error: unknown): string {
  return typeof error === "string" ? error : error instanceof Error ? error.message : JSON.stringify(error);
}
