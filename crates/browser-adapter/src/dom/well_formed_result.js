function assertWellFormedResult(value, depth = 0) {
  if (depth > 64) throw new Error('E_BROWSER_RESULT_DEPTH');
  if (typeof value === 'string' && !value.isWellFormed()) throw new Error('E_BROWSER_UTF16');
  if (value && typeof value === 'object') {
    for (const key of Object.keys(value)) {
      if (!key.isWellFormed()) throw new Error('E_BROWSER_UTF16');
      assertWellFormedResult(value[key], depth + 1);
    }
  }
}
