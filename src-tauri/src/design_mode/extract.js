function tuicExtract() {
  const el = this;
  const doc = el.ownerDocument;
  const escape = (value) => globalThis.CSS?.escape?.(value) ?? value.replace(/[^a-zA-Z0-9_-]/g, (char) => `\\${char}`);
  const stableClass = (value) => !/^(?:css-|sc-|_)[a-zA-Z0-9_-]*[a-fA-F0-9]{6,}$/.test(value) && !/^[a-fA-F0-9]{8,}$/.test(value);
  const tag = (node) => node.tagName.toLowerCase();
  const unique = (selector) => {
    try { return doc.querySelectorAll(selector).length === 1; }
    catch { return false; }
  };
  const nth = (node) => {
    let index = 1;
    for (let sibling = node.previousElementSibling; sibling; sibling = sibling.previousElementSibling) {
      if (sibling.tagName === node.tagName) index += 1;
    }
    return `${tag(node)}:nth-of-type(${index})`;
  };
  const fragment = (node) => {
    if (node.id && unique(`#${escape(node.id)}`)) return `#${escape(node.id)}`;
    for (const name of node.classList) {
      if (stableClass(name) && unique(`${tag(node)}.${escape(name)}`)) return `${tag(node)}.${escape(name)}`;
    }
    return nth(node);
  };
  const chain = [];
  for (let node = el; node && node.nodeType === 1; node = node.parentElement) {
    chain.unshift(node);
  }
  let selector = fragment(el);
  for (let index = chain.length - 2; !unique(selector) && index >= 0; index -= 1) {
    selector = `${fragment(chain[index])} > ${selector}`;
  }
  const path = (max) => chain.slice(-max).map(fragment).join(" > ");
  const attributes = Object.fromEntries(Array.from(el.attributes, (attr) => [attr.name, attr.value]).slice(0, 32));
  const fiberKey = Object.keys(el).find((key) => key.startsWith("__reactFiber$"));
  const fiber = fiberKey ? el[fiberKey] : undefined;
  const source = {};
  const computed = doc.defaultView?.getComputedStyle(el);
  const styleNames = ["color", "backgroundColor", "fontFamily", "fontSize", "fontWeight", "display", "position", "margin", "padding", "border", "width", "height"];
  const styles = Object.fromEntries(styleNames.map((name) => [name, computed?.[name] ?? ""]));
  const box = el.getBoundingClientRect();
  if (fiber?._debugStack?.stack) source.reactStack = fiber._debugStack.stack;
  if (fiber?._debugSource) source.reactSource = fiber._debugSource;
  if (el.__vueParentComponent?.type?.__file) source.vueFile = el.__vueParentComponent.type.__file;
  if (el.__svelte_meta?.loc) source.svelteLoc = el.__svelte_meta.loc;
  return {
    url: doc.location.href,
    tagName: tag(el),
    selector,
    elementPath: path(6),
    fullPath: path(20),
    nearbyText: (el.parentElement?.textContent ?? el.textContent ?? "").trim().slice(0, 500),
    attributes,
    htmlSnippet: el.outerHTML.slice(0, 2048),
    textContent: (el.textContent ?? "").slice(0, 2048),
    styles,
    rect: { x: box.x, y: box.y, width: box.width, height: box.height },
    source,
  };
}
