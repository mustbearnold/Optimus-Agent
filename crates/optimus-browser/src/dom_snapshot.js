// DOM snapshot: extract interactable elements with bounding boxes
// This file is included at compile time via include_str!("dom_snapshot.js")
// and evaluated via CDP Runtime.evaluate.

(() => {
    const interactiveSelectors = [
        'a', 'button', 'input', 'select', 'textarea',
        '[role="button"]', '[role="link"]', '[role="checkbox"]',
        '[role="radio"]', '[role="tab"]', '[role="menuitem"]',
        '[onclick]', '[tabindex]:not([tabindex="-1"])'
    ];
    const elements = document.querySelectorAll(interactiveSelectors.join(','));
    const results = [];
    let index = 0;
    for (const el of elements) {
        const rect = el.getBoundingClientRect();
        // Skip invisible / zero-size / off-screen elements
        if (rect.width < 2 || rect.height < 2) continue;
        if (rect.top > window.innerHeight || rect.left > window.innerWidth) continue;
        if (rect.bottom < 0 || rect.right < 0) continue;
        const text = (el.textContent || '').trim().slice(0, 80);
        const tag = el.tagName.toLowerCase();
        results.push({
            index: index++,
            tag: tag,
            text: text,
            bounds: { x: rect.x, y: rect.y, width: rect.width, height: rect.height },
            interactive: true
        });
    }
    return JSON.stringify(results);
})();
