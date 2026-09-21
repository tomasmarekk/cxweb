function (root, excluded = null) {
  // Preserve text-node whitespace. innerText folds consecutive spaces under
  // normal CSS, including indentation inside a serialized custom tool input.
  // This projects the visible DOM; it does not reconstruct Markdown source.
  const parts = [];
  let length = 0, nodes = 0;
  const append = text => {
    length += text.length;
    if (length > 4 * 1024 * 1024) throw new Error('E_PAYLOAD_LIMIT');
    parts.push(text);
  };
  const block = node => node.nodeType === 1 && ['DIV', 'P', 'PRE', 'LI'].includes(node.tagName);
  const visit = (node, depth) => {
    if (node === excluded) return false;
    if (++nodes > 65536 || depth > 64) throw new Error('E_PAYLOAD_LIMIT');
    if (node.nodeType === 3) { append(node.textContent); return true; }
    if (node.nodeType !== 1) return false;
    const style = getComputedStyle(node);
    if (node.hidden || node.getAttribute('aria-hidden') === 'true'
        || node.matches('button, [role="button"], script, style')
        || style.display === 'none' || ['hidden', 'collapse'].includes(style.visibility)) return false;
    if (node.tagName === 'BR') { append('\n'); return true; }
    let previous = null;
    for (const child of node.childNodes) {
      const separator = previous && (block(child) || block(previous));
      const index = parts.length, before = length;
      if (separator) append('\n');
      if (visit(child, depth + 1)) previous = child;
      else if (separator) { parts.length = index; length = before; }
    }
    return true;
  };
  if (root) visit(root, 0);
  return parts.join('');
}
