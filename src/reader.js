(() => {
    // The readable part of the page, as plain structure: headings,
    // paragraphs, lists, quotes and code, with their links and emphasis.
    // Nothing of the page's own markup, styles or scripts is carried over;
    // every text is escaped and every link is an absolute web address.
    const escape = text => text.replace(/[&<>"]/g, c => ({'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;'}[c]));
    const skipped = 'nav,aside,footer,header,form,script,style,noscript,template,iframe,button,select,svg,figure>img,[hidden],[aria-hidden="true"]';
    const hidden = element => {
        const style = getComputedStyle(element);
        return style.display === 'none' || style.visibility === 'hidden';
    };
    const words = element => (element.innerText || '').trim().split(/\s+/).filter(Boolean).length;
    // The article: the page's own, or the block that holds the most of its
    // paragraphs' words.
    let root = document.querySelector('article, [role="article"], main article') || document.querySelector('main, [role="main"]');
    if (!root || words(root) < 120) {
        const score = new Map();
        for (const paragraph of document.querySelectorAll('p')) {
            const count = words(paragraph);
            if (count < 12 || paragraph.closest(skipped)) continue;
            for (let parent = paragraph.parentElement, share = count; parent && parent !== document.documentElement; parent = parent.parentElement, share /= 2) {
                score.set(parent, (score.get(parent) || 0) + share);
            }
        }
        let best = 0;
        for (const [element, value] of score) {
            if (value > best) { best = value; root = element; }
        }
    }
    if (!root) return JSON.stringify({error: 'nothing to read'});
    const inline = node => {
        let out = '';
        for (const child of node.childNodes) {
            if (child.nodeType === Node.TEXT_NODE) { out += escape(child.textContent); continue; }
            if (child.nodeType !== Node.ELEMENT_NODE || child.matches(skipped) || hidden(child)) continue;
            const tag = child.tagName.toLowerCase();
            const inner = inline(child);
            if (tag === 'a' && /^https?:/.test(child.href)) out += `<a href="${escape(child.href)}">${inner}</a>`;
            else if (['em', 'i', 'strong', 'b', 'code', 'sub', 'sup'].includes(tag)) out += `<${tag}>${inner}</${tag}>`;
            else if (tag === 'br') out += '<br>';
            else out += inner;
        }
        return out;
    };
    const blocks = [];
    const walk = element => {
        for (const child of element.children) {
            if (child.matches(skipped) || hidden(child)) continue;
            const tag = child.tagName.toLowerCase();
            if (/^h[1-6]$/.test(tag) || tag === 'p' || tag === 'blockquote' || tag === 'figcaption') {
                const text = inline(child).trim();
                if (text) blocks.push(`<${tag}>${text}</${tag}>`);
            } else if (tag === 'pre') {
                blocks.push(`<pre>${escape(child.textContent)}</pre>`);
            } else if (tag === 'ul' || tag === 'ol') {
                const items = [...child.children].filter(item => item.tagName === 'LI' && !hidden(item)).map(item => `<li>${inline(item).trim()}</li>`).filter(item => item !== '<li></li>');
                if (items.length) blocks.push(`<${tag}>${items.join('')}</${tag}>`);
            } else {
                walk(child);
            }
        }
    };
    walk(root);
    const html = blocks.join('\n').slice(0, 4000000);
    if (html.replace(/<[^>]*>/g, '').trim().split(/\s+/).length < 40) return JSON.stringify({error: 'nothing to read'});
    return JSON.stringify({title: document.title.slice(0, 300), site: location.hostname, html});
})()
