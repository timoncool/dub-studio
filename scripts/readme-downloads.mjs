// Checks (default) or rewrites (--write) the download tables of README.md and its five
// translations against manifest() of crates/dub-server/src/setup.rs.
// A table sits between <!-- downloads:start --> and <!-- downloads:end -->.
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const START = "<!-- downloads:start -->";
const END = "<!-- downloads:end -->";

const FILES = {
  en: "README.md",
  ru: "README.ru.md",
  zh: "README.zh.md",
  es: "README.es.md",
  fr: "README.fr.md",
  pt: "README.pt.md",
};

const TEXT = {
  en: {
    head: ["Component", "Need", "Files (direct links)", "Size", "Put it in"],
    need: { required: "required", recommended: "recommended", optional: "optional" },
    none: "as is, to the path after the arrow",
    zipflat: "unzip the files, without subfolders, into",
    ziptree: "unzip with its folder tree into",
    zippick: "take ffmpeg.exe and ffprobe.exe from the archive into",
    wheeldlls: "take every .dll of the archive into",
  },
  ru: {
    head: ["Компонент", "Нужен", "Файлы (прямые ссылки)", "Размер", "Куда положить"],
    need: { required: "обязателен", recommended: "рекомендуется", optional: "по желанию" },
    none: "как есть, по пути после стрелки",
    zipflat: "распаковать файлы без подпапок в",
    ziptree: "распаковать с деревом папок в",
    zippick: "взять ffmpeg.exe и ffprobe.exe из архива в",
    wheeldlls: "взять все .dll из архива в",
  },
  zh: {
    head: ["组件", "必要性", "文件（直接链接）", "大小", "存放位置"],
    need: { required: "必需", recommended: "推荐", optional: "可选" },
    none: "原样放入，路径见箭头后",
    zipflat: "解压文件（不含子文件夹）到",
    ziptree: "保留文件夹结构解压到",
    zippick: "从压缩包中取出 ffmpeg.exe 和 ffprobe.exe 放入",
    wheeldlls: "取出压缩包中所有 .dll 放入",
  },
  es: {
    head: ["Componente", "Necesidad", "Archivos (enlaces directos)", "Tamaño", "Dónde ponerlo"],
    need: { required: "obligatorio", recommended: "recomendado", optional: "opcional" },
    none: "tal cual, en la ruta tras la flecha",
    zipflat: "descomprimir los archivos, sin subcarpetas, en",
    ziptree: "descomprimir con su árbol de carpetas en",
    zippick: "tomar ffmpeg.exe y ffprobe.exe del archivo y ponerlos en",
    wheeldlls: "tomar todos los .dll del archivo y ponerlos en",
  },
  fr: {
    head: ["Composant", "Besoin", "Fichiers (liens directs)", "Taille", "Où le placer"],
    need: { required: "obligatoire", recommended: "recommandé", optional: "facultatif" },
    none: "tel quel, au chemin indiqué après la flèche",
    zipflat: "décompresser les fichiers, sans sous-dossiers, dans",
    ziptree: "décompresser avec son arborescence dans",
    zippick: "prendre ffmpeg.exe et ffprobe.exe de l'archive et les mettre dans",
    wheeldlls: "prendre tous les .dll de l'archive et les mettre dans",
  },
  pt: {
    head: ["Componente", "Necessidade", "Arquivos (links diretos)", "Tamanho", "Onde colocar"],
    need: { required: "obrigatório", recommended: "recomendado", optional: "opcional" },
    none: "como está, no caminho após a seta",
    zipflat: "descompactar os arquivos, sem subpastas, em",
    ziptree: "descompactar com a árvore de pastas em",
    zippick: "pegar ffmpeg.exe e ffprobe.exe do arquivo e colocar em",
    wheeldlls: "pegar todos os .dll do arquivo e colocar em",
  },
};

const CASTING = {
  en: "Casting models (faces and voice)",
  ru: "Модели кастинга (лица и голос)",
  zh: "选角模型（人脸与声音）",
  es: "Modelos de casting (caras y voz)",
  fr: "Modèles de casting (visages et voix)",
  pt: "Modelos de casting (rostos e voz)",
};

const NAMES = {
  higgs: "Higgs Audio v3 Q8_0",
  "higgs-engine": "audiocpp_engine.dll (Higgs engine)",
  gemma: "Gemma-4 12B QAT q4_0 + vision",
  "gemma-q5_0": "Gemma-4 12B Q5_K_M + vision",
  "gemma-q6_k": "Gemma-4 12B Q6_K + vision",
  "gemma-q8_0": "Gemma-4 12B Q8_0 + vision",
  parakeet: "Parakeet-TDT 0.6B v3 Q8_0 (transcribe.cpp)",
  "transcribe-engine": "transcribe.cpp engine (CUDA)",
  "transcribe-engine-cpu": "transcribe.cpp engine (CPU)",
  "higgs-q6_k": "Higgs Audio v3 Q6_K",
  "higgs-q4_k_m": "Higgs Audio v3 Q4_K_M",
  drama3: "Fish Audio Drama 3 Q4 + drama_engine (24 GB+ GPU)",
  "parakeet-ultra": "Parakeet Ultra 0.6B Q8_0 (Moondream, transcribe.cpp)",
  "crisperwhisper-turbo": "CrisperWhisper 2.0 turbo (audio.cpp)",
  "qwen3-tts-engine": "qwen.dll + ggml (Qwen3-TTS engine)",
  "qwen3-tts": "Qwen3-TTS 1.7B Q4_K_M",
  "qwen3-tts-q8_0": "Qwen3-TTS 1.7B Q8_0",
  "qwen3-tts-ru-stress": "Qwen3-TTS Russian stress",
  supertonic3: "Supertonic 3 int8",
  ruaccent: "RUAccent",
  "ruaccent-omograph-big": "RUAccent omograph big_poetry int8",
  langid: "VoxLingua107 language id (transcribe.cpp)",
  sortformer: "Nemotron 3 Diarization",
  roformer: "Mel-Band Roformer voc_fv6 Q8_0",
  "roformer-q5": "Mel-Band Roformer voc_fv6 Q5_0",
  "roformer-q4": "Mel-Band Roformer voc_fv6 Q4_0",
  casting: CASTING,
  "bsroformer-engine": "BSRoformer.cpp (CUDA)",
  "bsroformer-engine-cpu": "BSRoformer.cpp (CPU)",
  llama: "llama.cpp server (CUDA)",
  onnxruntime: "ONNX Runtime",
  "onnxruntime-gpu": "ONNX Runtime GPU (CUDA)",
  ffmpeg: "FFmpeg (static build)",
  ytdlp: "yt-dlp + deno",
  "cuda-runtime": "CUDA runtime (cudart, cuBLAS, cuFFT)",
  cudnn: "cuDNN 9",
};

// The README describes the Windows build: platform::X is read from the Windows platform module.
function windowsModule(src) {
  return src.slice(src.indexOf("#[cfg(windows)]\nmod platform"), src.indexOf("#[cfg(not(windows))]\nmod platform"));
}

// The body of `pub const X: Parts = Parts { ... };` of the Windows platform module.
function windowsParts(src, x) {
  const windows = windowsModule(src);
  const at = windows.indexOf(`pub const ${x}: Parts = Parts {`);
  if (at < 0) throw new Error(`platform::${x} Parts not found in the Windows platform module`);
  return windows.slice(at, windows.indexOf("\n    };", at));
}

// The body of `const NAME: &[FileSpec] = &[...];` with every `platform::X` replaced by the Windows definition of X.
function constArray(src, name) {
  const at = src.indexOf(`const ${name}: &[FileSpec] = &[`);
  if (at < 0) throw new Error(`const ${name} not found in setup.rs`);
  const body = src.slice(at, src.indexOf("\n];", at));
  const windows = windowsModule(src);
  return body.replace(/platform::(\w+)/g, (_, x) => {
    const def = windows.match(new RegExp(`pub const ${x}: FileSpec = (FileSpec \\{[^\\n]*\\});`))?.[1];
    if (!def) throw new Error(`platform::${x} not found in the Windows platform module`);
    return def;
  });
}

function readManifest() {
  const src = readFileSync(join(root, "crates/dub-server/src/setup.rs"), "utf8").replace(/\r\n/g, "\n");
  const consts = {};
  for (const m of src.matchAll(/const (\w+): &str =\s*"([^"]+)";/g)) consts[m[1]] = m[2];
  for (const m of windowsModule(src).matchAll(/const (\w+): &str =\s*"([^"]+)";/g)) consts[m[1]] = m[2];
  // const NAME: FileSpec = FileSpec { ... }; named in `files: &[NAME, ...]`
  const specs = {};
  for (const m of src.matchAll(/const (\w+): FileSpec = (FileSpec \{[^\n]*\});/g)) specs[m[1]] = m[2];
  const at = src.indexOf("pub fn manifest()");
  if (at < 0) throw new Error("manifest() not found in setup.rs");
  const body = src.slice(at, src.indexOf("\n}\n", at));
  const parts = body.split(/\n {8}Component \{/).slice(1);
  const num = (s) => Number(s.replace(/_/g, ""));
  return parts.map((p) => {
    const id = p.match(/id: "([^"]+)"/)?.[1];
    const requirement = p.match(/requirement: Requirement::(\w+)/)?.[1]?.toLowerCase();
    const delivery = p.match(/delivery: Delivery::(\w+)/)?.[1];
    const size = num(p.match(/delivery: Delivery::\w+,\s*(?:\/\/[^\n]*\n\s*)*size: ([\d_]+)/)?.[1] ?? "0");
    if (!id || !requirement || !delivery) throw new Error("cannot parse a manifest component: " + p.slice(0, 120));
    // files: CONST — a const array of setup.rs; files: platform::X.files — the Windows Parts X
    const listed = p.match(/files: ([A-Z][A-Z0-9_]+),/)?.[1];
    const parts = p.match(/files: platform::(\w+)\.files,/)?.[1];
    const own = p.replace(/files: &\[([A-Z0-9_, ]+)\]/, (_, list) => "files: &[" + list.split(",").map((x) => specs[x.trim()] ?? x).join(", ") + "]");
    const scope = listed ? constArray(src, listed) : parts ? windowsParts(src, parts) : own;
    const files = [...scope.matchAll(/FileSpec \{ url: (?:"([^"]+)"|(\w+)), dest_rel: "([^"]+)", size: ([\d_]+),(?: sha256: "[0-9a-f]{64}",)? extract: Extract::(\w+) \}/g)].map((f) => {
      const url = f[1] ?? consts[f[2]];
      if (!url) throw new Error(`unknown URL constant ${f[2]} in component ${id}`);
      const extract = { None: "none", Flat: "zipflat", Pick: "zippick", Tree: "ziptree", Libs: "wheeldlls" }[f[5]];
      if (!extract) throw new Error(`unknown Extract::${f[5]} in component ${id}`);
      return { url, destRel: f[3], size: num(f[4]), extract };
    });
    return { id, requirement, delivery, size: size || files.reduce((s, f) => s + f.size, 0), files };
  });
}

function human(bytes) {
  if (bytes >= 1e9) return (bytes / 1e9).toFixed(1) + " GB";
  if (bytes >= 1e6) return Math.round(bytes / 1e6) + " MB";
  return Math.max(1, Math.round(bytes / 1e3)) + " KB";
}

function winPath(rel) {
  return rel.replace(/\//g, "\\");
}

function folderOf(rel) {
  const i = rel.lastIndexOf("/");
  return (i < 0 ? "" : rel.slice(0, i + 1)).replace(/\//g, "\\");
}

function table(manifest, lang) {
  const T = TEXT[lang];
  const rows = [`| ${T.head.join(" | ")} |`, `|${T.head.map(() => "---").join("|")}|`];
  for (const c of manifest) {
    if (c.delivery !== "Download") continue;
    const name = NAMES[c.id];
    if (!name) throw new Error(`component "${c.id}" has no name in scripts/readme-downloads.mjs`);
    const label = typeof name === "string" ? name : name[lang];
    const links = c.files.map((f) => `[${f.url.split("/").pop()}](${f.url})${f.extract === "none" ? ` → \`${winPath(f.destRel)}\`` : ""}`).join("<br>");
    const places = [...new Set(c.files.map((f) => (f.extract === "none" ? T.none : `${T[f.extract]} \`${folderOf(f.destRel)}\``)))];
    rows.push(`| ${label} | ${T.need[c.requirement]} | ${links} | ${human(c.size)} | ${places.join("<br>")} |`);
  }
  return rows.join("\n");
}

function blockOf(text) {
  const a = text.indexOf(START);
  const b = text.indexOf(END);
  if (a < 0 || b < a) return null;
  return { a: a + START.length, b };
}

const manifest = readManifest();
const write = process.argv.includes("--write");
const urlsOf = (s) => new Set([...s.matchAll(/\]\((https?:\/\/[^)]+)\)/g)].map((m) => m[1]));
const wanted = new Set(manifest.flatMap((c) => (c.delivery === "Download" ? c.files.map((f) => f.url) : [])));
let failed = false;

for (const [lang, file] of Object.entries(FILES)) {
  const path = join(root, file);
  const raw = readFileSync(path, "utf8");
  const eol = raw.includes("\r\n") ? "\r\n" : "\n";
  const text = raw.replace(/\r\n/g, "\n");
  const blk = blockOf(text);
  if (!blk) {
    console.error(`[ERROR] ${file}: no ${START} ... ${END} block`);
    failed = true;
    continue;
  }
  const fresh = "\n" + table(manifest, lang) + "\n";
  if (write) {
    writeFileSync(path, (text.slice(0, blk.a) + fresh + text.slice(blk.b)).replace(/\n/g, eol));
    console.log(`[OK] ${file}: table written`);
    continue;
  }
  const have = text.slice(blk.a, blk.b);
  for (const c of manifest) {
    if (c.delivery !== "Download") continue;
    for (const f of c.files) {
      if (f.extract === "none" && !have.includes(`${f.url.split("/").pop()}](${f.url}) → \`${winPath(f.destRel)}\``)) {
        console.error(`[ERROR] ${file}: no destination ${winPath(f.destRel)} next to ${f.url}`);
        failed = true;
      }
    }
  }
  const present = urlsOf(have);
  const missing = [...wanted].filter((u) => !present.has(u));
  const extra = [...present].filter((u) => !wanted.has(u));
  if (missing.length || extra.length || have !== fresh) {
    console.error(`[ERROR] ${file}: the table differs from setup.rs manifest()`);
    for (const u of missing) console.error(`  missing in README: ${u}`);
    for (const u of extra) console.error(`  not in manifest: ${u}`);
    if (!missing.length && !extra.length) console.error("  links match, sizes, names or folders differ; run with --write");
    failed = true;
  } else {
    console.log(`[OK] ${file}: ${wanted.size} links match the manifest`);
  }
}
process.exit(failed ? 1 : 0);
