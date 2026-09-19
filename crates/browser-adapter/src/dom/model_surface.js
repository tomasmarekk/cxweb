function () {
  const visible = element => element instanceof HTMLElement && element.getClientRects().length > 0;
  const roots = [...document.querySelectorAll('[data-testid="composer-intelligence-picker-content"], [role="menu"], [role="group"], [role="listbox"], [role="dialog"]')].filter(root =>
    visible(root) && root.querySelector('[role="menuitemradio"], [data-model-reasoning-effort-slider]')
  );
  const selector = '[role="menuitem"], [role="menuitemradio"], [role="option"], [data-testid*="model-option" i], [data-testid^="model-switcher-"]';
  const nodes = [...new Set(roots.flatMap(root => [...root.querySelectorAll(selector)]))].filter(node =>
    visible(node) && node.getAttribute('data-testid') !== 'model-switcher-dropdown-button'
  );
  let candidates = nodes.map((node, index) => {
    const label = (node.textContent ?? '').replace(/\s+/g, ' ').trim().slice(0, 120);
    const testid = (node.getAttribute('data-testid') ?? '').trim();
    const aria = (node.getAttribute('aria-label') ?? '').trim();
    const identity = (testid || aria || `${node.getAttribute('role') ?? 'item'}:${index}:${label}`).slice(0, 240);
    return {
      label,
      identity,
      selected: node.getAttribute('aria-checked') === 'true' || node.getAttribute('aria-selected') === 'true' || node.dataset.state === 'checked'
    };
  }).filter(candidate => candidate.label && candidate.identity);
  const sliderContainer = [...document.querySelectorAll('[data-model-reasoning-effort-slider]')].filter(visible).at(-1);
  const slider = sliderContainer?.querySelector('[role="slider"]');
  const menuOptions = candidates;
  const min = Number(slider?.getAttribute('aria-valuemin'));
  const max = Number(slider?.getAttribute('aria-valuemax'));
  const value = Number(slider?.getAttribute('aria-valuenow'));
  const rangeValid = !!slider && ['aria-valuemin', 'aria-valuemax', 'aria-valuenow'].every(name => slider.hasAttribute(name))
    && Number.isSafeInteger(min) && Number.isSafeInteger(max) && Number.isSafeInteger(value)
    && min <= value && value <= max && max - min >= 0 && max - min < 5;
  if (rangeValid) {
    const composers = [...document.querySelectorAll('[data-testid="prompt-textarea"], #prompt-textarea, [contenteditable="true"][data-lexical-editor="true"]')].filter(visible);
    if (composers.length !== 1) throw new Error('E_MODEL_MENU');
    const form = composers[0].closest('form');
    const controls = form ? [...form.querySelectorAll('button[aria-haspopup="menu"][data-tone="neutral"], button[data-testid="model-switcher-dropdown-button"][aria-haspopup="menu"]')].filter(visible) : [];
    const control = controls.at(-1);
    if (!control) throw new Error('E_MODEL_MENU');
    const selectedLabel = (control?.textContent ?? 'ChatGPT route').replace(/\s+/g, ' ').trim().slice(0, 90) || 'ChatGPT route';
    candidates = [{
      label: `${selectedLabel} · ${readEffortLabel(slider, min, max, value)}`,
      identity: `reasoning-slider:${min}:${max}:${value}`,
      selected: true
    }];
  }
  const unique = [];
  const seen = new Set();
  for (const candidate of candidates) {
    const key = `${candidate.identity}\u0000${candidate.label}`;
    if (!seen.has(key)) { seen.add(key); unique.push(candidate); }
  }
  const temporary = [...document.querySelectorAll('[data-testid*="temporary" i]')].some(visible);
  const switcher = document.querySelector('[data-testid="model-switcher-dropdown-button"]');
  const expanded = switcher?.getAttribute('aria-expanded');
  const testids = [...new Set([...document.querySelectorAll('[data-testid*="model" i]')]
    .filter(visible).map(node => node.getAttribute('data-testid')).filter(Boolean))].slice(0, 32);
  const roleCounts = new Map();
  for (const node of [...document.querySelectorAll('[role]')].filter(visible)) {
    const role = (node.getAttribute('role') ?? '').slice(0, 80);
    if (role) roleCounts.set(role, (roleCounts.get(role) ?? 0) + 1);
  }
  const roles = [...roleCounts].sort(([a], [b]) => a.localeCompare(b)).slice(0, 32)
    .map(([role, count]) => `${role}:${count}`);
  const composer = [...document.querySelectorAll('#prompt-textarea')].filter(visible)[0];
  const composerControls = [...(composer?.closest('form')?.querySelectorAll('button[aria-haspopup="menu"]') ?? [])]
    .filter(visible).slice(0, 8).map(node => [
      node.textContent.replace(/\s+/g, ' ').trim().slice(0, 60),
      node.getAttribute('data-tone') ?? '', node.getAttribute('aria-expanded') ?? ''
    ].join(' | ').slice(0, 120));
  return {
    candidates: unique,
    temporary_chat: temporary,
    diagnostic: {
      menu_options: menuOptions,
      effort_range: rangeValid ? { min, max, current: value } : null,
      switcher_expanded: expanded === 'true' ? true : expanded === 'false' ? false : null,
      visible_roots: roots.length,
      candidate_nodes: nodes.length,
      model_testids: testids,
      menu_states: roots.slice(0, 16).map(node => [
        ['open', 'closed'].includes(node.getAttribute('data-state')) ? node.getAttribute('data-state') : 'unknown',
        node.checkVisibility?.({ checkOpacity: true, checkVisibilityCSS: true }) ? 'visible' : 'hidden',
        getComputedStyle(node).pointerEvents === 'none' ? 'no-pointer' : 'pointer',
        node.getAnimations().some(animation => animation.playState === 'running') ? 'animating' : 'still'
      ].join(':')),
      composer_controls: composerControls,
      interaction_state: {
        ready: document.readyState === 'complete',
        focused: document.hasFocus?.() ?? false,
        visible: document.visibilityState === 'visible',
        disabled: [...(composer?.closest('form')?.querySelectorAll('button[aria-haspopup="menu"][data-tone="neutral"]') ?? [])].some(node => node.disabled || node.getAttribute('aria-disabled') === 'true'),
        inert: !!composer?.closest('[inert]'),
        active_menu_trigger: document.activeElement?.getAttribute('aria-haspopup') === 'menu'
      },
      visible_roles: roles
    }
  };
}
