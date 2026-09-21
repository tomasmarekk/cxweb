function (root) {
  // The current ChatGPT code viewer nests a CodeMirror PRE inside the outer
  // fenced block. Count independent outer blocks, not renderer implementation nodes.
  const blocks = root ? [...root.querySelectorAll('pre')].filter(block => !block.parentElement?.closest('pre')) : [];
  if (!blocks.length) return { text: readAnswerText(root), fenced: false };
  // Only one complete code block can carry the envelope. Never select a JSON
  // fragment from surrounding prose or choose between multiple candidates.
  if (blocks.length !== 1) return { text: '', fenced: true };
  const code = [...blocks[0].querySelectorAll('code')];
  if (code.length !== 1 || readAnswerText(root, blocks[0]).trim()) {
    return { text: '', fenced: true };
  }
  // Read the code itself, excluding ChatGPT's language/copy toolbar. JSON
  // escapes remain literal here; the contextual Rust decoder still validates
  // the entire envelope, nonce, tool identity, schema and requested tool choice.
  return { text: readAnswerText(code[0]), fenced: false };
}
