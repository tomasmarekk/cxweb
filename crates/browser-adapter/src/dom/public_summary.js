function (turn, answer) {
  if (!turn) return [];
  // Read rendered public status only. Never expand collapsed thinking or read
  // hidden application state. An answer wrapper cannot suppress its status child.
  const statusRows = [...turn.querySelectorAll('[data-streaming-response-status], [data-testid^="cot-v5"], .loading-shimmer-tertiary')];
  const completedRows = [...turn.querySelectorAll('button')].filter(node => /^Thought for\s+\d/.test(node.innerText.trim()));
  const rows = [...new Set([...statusRows, ...completedRows])]
    .filter(node => node instanceof HTMLElement && node.getClientRects().length > 0)
    .filter(node => !answer || (!node.contains(answer) && !answer.contains(node)));
  return [...new Set(rows
    .filter(node => !rows.some(parent => parent !== node && parent.contains(node)))
    .map(node => node.innerText.replace(/\s+/g, ' ').trim())
    .filter(text => text && text !== 'Answer now' && text.length <= 2048 && text.isWellFormed()))].slice(0, 64);
}
