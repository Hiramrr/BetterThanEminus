// Texto legible de los archivos de Eminus, para que el agente pueda estudiar con ellos.
//   PDF    → texto por página (unpdf / pdf.js). Un PDF escaneado no trae texto: `has_text: false`.
//   .ipynb → celdas de markdown y código, en orden (sin las salidas).
//   texto  → csv, txt, md, py, json, html… tal cual (html convertido a texto).
// Lo demás (docx, pptx, imágenes…) responde `unsupported_type`: usa la descarga binaria.
import { extOf, htmlToText } from "./eminus.mjs";

const TEXT_TYPES = new Set(["txt", "md", "csv", "tsv", "py", "r", "java", "c", "cpp", "h", "js", "ts", "sql", "json", "xml", "yml", "yaml", "tex", "html", "htm"]);
export const MAX_CHARS = 120_000; // por respuesta

export const canExtract = (name) => ["pdf", "ipynb"].includes(extOf(name)) || TEXT_TYPES.has(extOf(name));

/**
 * @returns {Promise<{ format: string, total_pages: number, from: number, to: number, pages: {page: number, text: string}[], has_text: boolean, truncated: boolean, next_from: number|null }>}
 * `from`/`to` son páginas (PDF), celdas (ipynb) o bloques de 20 000 caracteres (texto).
 */
export async function extractText(bytes, name, { from = 1, to } = {}) {
  const ext = extOf(name);
  let parts;
  let format;
  if (ext === "pdf") {
    format = "pdf";
    const { getDocumentProxy } = await import("unpdf");
    const pdf = await getDocumentProxy(new Uint8Array(bytes));
    const total = pdf.numPages;
    const last = Math.min(total, to ?? from + 19);
    parts = { total, get: async (n) => {
      const page = await pdf.getPage(n);
      const content = await page.getTextContent();
      let text = "";
      for (const item of content.items) text += (item.str ?? "") + (item.hasEOL ? "\n" : " ");
      return text.replace(/[ \t]+\n/g, "\n").replace(/[ \t]{2,}/g, " ").trim();
    }, last };
  } else if (ext === "ipynb") {
    format = "ipynb";
    let nb;
    try { nb = JSON.parse(Buffer.from(bytes).toString("utf8")); } catch { throw Object.assign(new Error("notebook ilegible"), { code: "unreadable_file" }); }
    const cells = (nb.cells || []).map((c) => {
      const src = Array.isArray(c.source) ? c.source.join("") : String(c.source || "");
      return c.cell_type === "code" ? "```python\n" + src.trim() + "\n```" : src.trim();
    }).filter(Boolean);
    parts = { total: cells.length, get: async (n) => cells[n - 1], last: Math.min(cells.length, to ?? from + 49) };
  } else if (TEXT_TYPES.has(ext)) {
    format = "text";
    let text = Buffer.from(bytes).toString("utf8");
    if (ext === "html" || ext === "htm") text = htmlToText(text);
    const size = 20_000;
    const chunks = [];
    for (let i = 0; i < text.length; i += size) chunks.push(text.slice(i, i + size));
    if (!chunks.length) chunks.push("");
    parts = { total: chunks.length, get: async (n) => chunks[n - 1], last: Math.min(chunks.length, to ?? from + 4) };
  } else {
    throw Object.assign(new Error("este tipo de archivo no se puede convertir a texto; descárgalo"), { code: "unsupported_type" });
  }

  const pages = [];
  let chars = 0;
  let truncated = false;
  let n = from;
  for (; n <= parts.last; n++) {
    const text = (await parts.get(n)) || "";
    if (chars + text.length > MAX_CHARS && pages.length) { truncated = true; break; }
    pages.push({ page: n, text });
    chars += text.length;
  }
  const to_ = pages.length ? pages[pages.length - 1].page : from - 1;
  return {
    format,
    total_pages: parts.total,
    from,
    to: to_,
    pages,
    has_text: pages.some((p) => p.text.trim().length > 0),
    truncated,
    next_from: to_ < parts.total ? to_ + 1 : null
  };
}
