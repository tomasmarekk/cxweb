function () {
  const visible = node => node instanceof HTMLElement && node.getClientRects().length > 0
    && node.checkVisibility({ checkOpacity: true, checkVisibilityCSS: true });
  // Match the observed service dialog, never conversation/model text. Read only:
  // do not dismiss it, retry the request, or attempt to bypass the restriction.
  return [...document.querySelectorAll('[role="dialog"], [role="alertdialog"]')]
    .filter(visible).some(dialog => {
      if (dialog.closest('[data-turn-id-container], [data-message-author-role]')) return false;
      const text = dialog.innerText.replace(/\s+/g, ' ').trim().replace(/[’‘]/g, "'");
      return text.startsWith('Too many requests ')
        && text.includes("You're making requests too quickly. We've temporarily limited access to your conversations to protect your data.")
        && text.includes('Please wait a few minutes before trying again.');
    });
}
