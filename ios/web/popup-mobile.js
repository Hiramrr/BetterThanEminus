// Textos propios de iOS, sin cambiar los controles de la extensión.
(() => {
  const labels = {
    "open-app": "Ver mi agenda",
    "bg-pick": "Elegir foto",
    "bg-clear": "Quitar",
    "bg-reset": "Restablecer",
    "bg-edit": "Acomodar la foto",
    "zoom-out": "−",
    "zoom-in": "+",
    "zoom-reset": "100%"
  };
  for (const [id, label] of Object.entries(labels)) {
    const button = document.getElementById(id);
    if (button) button.textContent = label;
  }
  for (const button of document.querySelectorAll("button")) {
    button.textContent = button.textContent.replace(/^\[\s*|\s*\]$/g, "");
  }
  for (const title of document.querySelectorAll("h2")) {
    const text = title.firstChild;
    if (text?.nodeType === Node.TEXT_NODE) text.textContent = text.textContent.replace(/^\w/, (c) => c.toUpperCase());
  }
  const fontHeading = document.getElementById("font")?.closest("section")?.querySelector("h2");
  if (fontHeading) fontHeading.textContent = "Tamaño del texto";
  const enabledLabel = document.querySelector('label[for="enabled"] span:last-of-type');
  if (enabledLabel) enabledLabel.textContent = "Usar la interfaz de Eminus";
})();
