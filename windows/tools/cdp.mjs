// Tiny Chrome DevTools Protocol client for checking the dock while Windows is locked.
// Usage:
//   node tools/cdp.mjs pages
//   node tools/cdp.mjs shot out.png [urlFilter]
//   node tools/cdp.mjs eval "expr" [urlFilter]
//   node tools/cdp.mjs move x y [urlFilter]
//   node tools/cdp.mjs click x y [urlFilter]
//   node tools/cdp.mjs rclick x y [urlFilter]

import fs from 'node:fs';

const PORT = 9222;
const [, , command, ...args] = process.argv;

const targets = await (await fetch(`http://127.0.0.1:${PORT}/json`)).json();

if (command === 'pages') {
    console.log(JSON.stringify(targets.map(t => ({ id: t.id, title: t.title, url: t.url })), null, 2));
    process.exit(0);
}

const filter =
    args.find(
        a =>
            typeof a === 'string' &&
            (a.includes('://') ||
                a.includes('.html') ||
                a.startsWith('about:') ||
                /^[0-9A-Fa-f]{32}$/.test(a)),
    ) ?? '';
const target =
    targets.find(t => t.type === 'page' && filter && (t.url === filter || t.id === filter)) ??
    targets.find(t => t.type === 'page' && t.url.includes(filter)) ??
    targets.find(t => t.type === 'page');

if (!target) {
    console.error('no page target');
    process.exit(1);
}

const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((resolve, reject) => {
    ws.onopen = resolve;
    ws.onerror = reject;
});

let nextId = 0;
const pending = new Map();
ws.onmessage = event => {
    const message = JSON.parse(event.data);
    if (message.id && pending.has(message.id)) {
        pending.get(message.id)(message);
        pending.delete(message.id);
    }
};

function send(method, params = {}) {
    return new Promise(resolve => {
        const id = ++nextId;
        pending.set(id, resolve);
        ws.send(JSON.stringify({ id, method, params }));
    });
}

async function evaluate(expression) {
    const response = await send('Runtime.evaluate', {
        expression,
        returnByValue: true,
        awaitPromise: true,
    });
    if (response.result?.exceptionDetails) {
        return { error: response.result.exceptionDetails.text ?? 'exception' };
    }
    return response.result?.result?.value;
}

async function mouse(type, x, y, button = 'none', clickCount = 0) {
    await send('Input.dispatchMouseEvent', {
        type,
        x,
        y,
        button,
        clickCount,
        buttons: button === 'left' ? 1 : button === 'right' ? 2 : 0,
    });
}

switch (command) {
    case 'shot': {
        const out = args[0] ?? 'dock.png';
        const response = await send('Page.captureScreenshot', {
            format: 'png',
            fromSurface: true,
        });
        fs.writeFileSync(out, Buffer.from(response.result.data, 'base64'));
        console.log(out);
        break;
    }
    case 'eval': {
        console.log(JSON.stringify(await evaluate(args[0] ?? '1'), null, 2));
        break;
    }
    case 'move': {
        await mouse('mouseMoved', Number(args[0]), Number(args[1]));
        break;
    }
    case 'nav': {
        const response = await send('Page.navigate', { url: args[0] });
        console.log(JSON.stringify(response.result ?? response.error ?? {}));
        break;
    }
    case 'click': {
        await mouse('mouseMoved', Number(args[0]), Number(args[1]));
        await new Promise(r => setTimeout(r, 80));
        await mouse('mousePressed', Number(args[0]), Number(args[1]), 'left', 1);
        await new Promise(r => setTimeout(r, 40));
        await mouse('mouseReleased', Number(args[0]), Number(args[1]), 'left', 1);
        break;
    }
    case 'rclick': {
        await mouse('mouseMoved', Number(args[0]), Number(args[1]));
        await new Promise(r => setTimeout(r, 80));
        await mouse('mousePressed', Number(args[0]), Number(args[1]), 'right', 1);
        await new Promise(r => setTimeout(r, 40));
        await mouse('mouseReleased', Number(args[0]), Number(args[1]), 'right', 1);
        break;
    }
    default:
        console.error(`unknown command: ${command}`);
        process.exit(1);
}

ws.close();
