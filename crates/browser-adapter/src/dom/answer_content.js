function (turn) {
  if (!turn) return { content: null, candidates: 0, intermediate: 0 };
  const candidates = [];
  let intermediate = 0;
  for (const block of turn.querySelectorAll('[data-message-author-role="assistant"] .markdown')) {
    if (block.closest('[data-streaming-response-status], [data-testid^="cot-v5"]')) {
      intermediate++;
    } else if (!block.parentElement?.closest('.markdown')) {
      candidates.push(block);
    }
  }
  return { content: candidates.length === 1 ? candidates[0] : null, candidates: candidates.length, intermediate };
}
