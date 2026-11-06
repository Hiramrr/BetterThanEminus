// OOXML contiene XML dentro de un ZIP. Solo leemos texto, sin ejecutar macros ni fórmulas.
import { posix } from "node:path";
import { unzipSync } from "fflate";
import { DOMParser } from "@xmldom/xmldom";

const nodes = (root, name) => Array.from(root.getElementsByTagNameNS("*", name));
const invalid = () => Object.assign(new Error("documento de Office ilegible o demasiado grande"), { code: "unreadable_file" });
const relationId = (node) => Array.from(node.attributes).find((a) => a.localName === "id" && a.namespaceURI?.endsWith("/relationships"))?.value;

export function officeParts(bytes, ext) {
  let expanded = 0;
  let entries = 0;
  const zip = unzipSync(new Uint8Array(bytes), { filter: (f) => {
    if (++entries > 10_000) throw invalid();
    if (!/^(word|ppt|xl)\/.*\.(xml|rels)$/.test(f.name)) return false;
    expanded += f.originalSize;
    if (f.originalSize > 8 * 1024 * 1024 || expanded > 32 * 1024 * 1024) throw invalid();
    return true;
  } });
  const xml = (path) => {
    if (!zip[path]) throw invalid();
    const text = Buffer.from(zip[path]).toString("utf8");
    if (/<!DOCTYPE/i.test(text)) throw invalid();
    return new DOMParser({ onError: (level) => { if (level !== "warning") throw invalid(); } }).parseFromString(text, "application/xml");
  };
  const rels = (path) => new Map(nodes(xml(posix.join(posix.dirname(path), "_rels", posix.basename(path) + ".rels")), "Relationship")
    .filter((r) => r.getAttribute("TargetMode") !== "External")
    .map((r) => {
      const target = r.getAttribute("Target");
      return [r.getAttribute("Id"), target.startsWith("/") ? target.slice(1) : posix.normalize(posix.join(posix.dirname(path), target))];
    }));
  const paragraphs = (doc) => nodes(doc, "p").map((p) => {
    let text = "";
    const walk = (n) => {
      if (n.localName === "t") { text += n.textContent; return; }
      if (n.localName === "tab") text += "\t";
      if (n.localName === "br") text += "\n";
      for (const c of Array.from(n.childNodes || [])) walk(c);
    };
    walk(p);
    return text;
  }).join("\n");
  const chunks = (text) => {
    const result = [];
    for (let i = 0; i < text.length; i += 20_000) result.push(text.slice(i, i + 20_000));
    return result.length ? result : [""];
  };
  if (ext === "docx") return chunks(paragraphs(xml("word/document.xml")));
  if (ext === "pptx") {
    const path = "ppt/presentation.xml";
    const links = rels(path);
    return nodes(xml(path), "sldId").flatMap((slide, i) => {
      const target = links.get(relationId(slide));
      return chunks(paragraphs(xml(target))).map((text) => "Diapositiva " + (i + 1) + "\n" + text);
    });
  }
  const path = "xl/workbook.xml";
  const links = rels(path);
  const shared = zip["xl/sharedStrings.xml"] ? nodes(xml("xl/sharedStrings.xml"), "si").map((s) => nodes(s, "t").map((t) => t.textContent).join("")) : [];
  return nodes(xml(path), "sheet").flatMap((sheet) => {
    const target = links.get(relationId(sheet));
    const rows = nodes(xml(target), "row").map((row) => nodes(row, "c").map((cell) => {
      const type = cell.getAttribute("t");
      const v = nodes(cell, "v")[0]?.textContent;
      let value = v ?? "";
      if (type === "s") value = shared[Number(v)] ?? "";
      else if (type === "inlineStr") value = nodes(cell, "t").map((t) => t.textContent).join("");
      else if (v === undefined && nodes(cell, "f")[0]) value = "=" + nodes(cell, "f")[0].textContent;
      return cell.getAttribute("r") + "=" + value;
    }).join("\t")).join("\n");
    return chunks(rows).map((text) => "Hoja: " + sheet.getAttribute("name") + "\n" + text);
  });
}
