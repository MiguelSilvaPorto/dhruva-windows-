// Sweep the pointer across the dock while recording a screencast.
// Usage: node tools/screencast-sweep.mjs <tabId> <outDir>
import fs from 'node:fs';
import path from 'node:path';

const [, , tabId, outDirArg] = process.argv;
const port = 9222;
const outDir = outDirArg ?? 'C:\\dhruva-build\\sweep';
fs.mkdirSync(outDir, { recursive: true });

const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
const target = targets.find(t => t.id === tabId);
if (!target) {
    console.error('tab not found');
    process.exit(1);
}

const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((resolve, reject) => {
    ws.onopen = resolve;
    ws.onerror = reject;
});

let nextId = 0;
const pending = new Map();
let frame = 0;

ws.onmessage = event => {
    const message = JSON.parse(event.data);
    if (message.id && pending.has(message.id)) {
        pending.get(message.id)(message);
        pending.delete(message.id);
    }
    if (message.method === 'Page.screencastFrame') {
        const name = `frame-${String(frame).padStart(4, '0')}.png`;
        fs.writeFileSync(path.join(outDir, name), Buffer.from(message.params.data, 'base64'));
        frame++;
        ws.send(
            JSON.stringify({
                id: ++nextId,
                method: 'Page.screencastFrameAck',
                params: { sessionId: message.params.sessionId },
            }),
        );
    }
};

function send(method, params = {}) {
    return new Promise(resolve => {
        const id = ++nextId;
        pending.set(id, resolve);
        ws.send(JSON.stringify({ id, method, params }));
    });
}

async function mouse(x, y) {
    await send('Input.dispatchMouseEvent', {
        type: 'mouseMoved',
        x,
        y,
        button: 'none',
        buttons: 0,
    });
}

await send('Page.enable');
await send('Page.startScreencast', {
    format: 'png',
    maxWidth: 1600,
    maxHeight: 640,
    everyNthFrame: 1,
});

await mouse(380, 450);
await new Promise(r => setTimeout(r, 200));
for (let x = 380; x <= 900; x += 4) {
    await mouse(x, 605);
    await new Promise(r => setTimeout(r, 12));
}
await new Promise(r => setTimeout(r, 500));
for (let x = 900; x >= 380; x -= 4) {
    await mouse(x, 605);
    await new Promise(r => setTimeout(r, 12));
}
await new Promise(r => setTimeout(r, 500));
await mouse(500, 450);
await new Promise(r => setTimeout(r, 900));

await send('Page.stopScreencast');
ws.close();
console.log(JSON.stringify({ frames: frame, outDir }));
