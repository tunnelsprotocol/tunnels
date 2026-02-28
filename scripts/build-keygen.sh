#!/bin/bash
# Build the tunnels-keygen.html file with inlined WASM SDK.
#
# Prerequisites:
#   - wasm-pack: cargo install wasm-pack
#   - base64 command (standard on macOS/Linux)
#
# Output: tunnels-keygen.html in the workspace root

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
SDK_DIR="$ROOT_DIR/tunnels-sdk"
OUTPUT_FILE="$ROOT_DIR/tunnels-keygen.html"

echo "Building WASM SDK..."
cd "$SDK_DIR"

# Build with wasm-pack for web target
wasm-pack build --target web --release --out-dir pkg

# Extract the WASM binary and encode as base64
WASM_FILE="$SDK_DIR/pkg/tunnels_sdk_bg.wasm"
JS_FILE="$SDK_DIR/pkg/tunnels_sdk.js"

if [ ! -f "$WASM_FILE" ] || [ ! -f "$JS_FILE" ]; then
    echo "Error: WASM build failed - missing output files"
    exit 1
fi

echo "Encoding WASM as base64..."
WASM_BASE64=$(base64 -i "$WASM_FILE" | tr -d '\n')

echo "Generating HTML..."

# Read the JS bindings
JS_CONTENT=$(cat "$JS_FILE")

# Generate the HTML file
cat > "$OUTPUT_FILE" << 'HTMLHEAD'
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Tunnels Protocol Key Generator</title>
    <style>
        * { box-sizing: border-box; margin: 0; padding: 0; }
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, sans-serif;
            background: #1a1a2e;
            color: #eee;
            min-height: 100vh;
            padding: 2rem;
        }
        .container { max-width: 800px; margin: 0 auto; }
        h1 { color: #4cc9f0; margin-bottom: 0.5rem; }
        .subtitle { color: #888; margin-bottom: 2rem; }
        .card {
            background: #16213e;
            border-radius: 12px;
            padding: 2rem;
            margin-bottom: 1.5rem;
            border: 1px solid #0f3460;
        }
        .warning {
            background: #4a1942;
            border-color: #e94560;
            color: #ffb8b8;
        }
        .warning h3 { color: #e94560; margin-bottom: 0.5rem; }
        button {
            background: #4cc9f0;
            color: #1a1a2e;
            border: none;
            padding: 1rem 2rem;
            font-size: 1.1rem;
            font-weight: bold;
            border-radius: 8px;
            cursor: pointer;
            transition: background 0.2s;
            margin-right: 0.5rem;
            margin-bottom: 0.5rem;
        }
        button:hover { background: #3db8df; }
        button:disabled { background: #555; cursor: not-allowed; }
        .secondary-btn {
            background: #0f3460;
            color: #4cc9f0;
        }
        .secondary-btn:hover { background: #1a4980; }
        input[type="password"], input[type="file"] {
            background: #0f3460;
            border: 1px solid #4cc9f0;
            color: #eee;
            padding: 0.75rem 1rem;
            border-radius: 6px;
            font-size: 1rem;
            width: 100%;
            margin-bottom: 1rem;
        }
        input[type="file"] { padding: 0.5rem; }
        label { display: block; color: #4cc9f0; font-weight: bold; margin-bottom: 0.5rem; font-size: 0.9rem; }
        .key-display { display: none; }
        .key-display.visible { display: block; }
        .key-field { margin-bottom: 1.5rem; }
        .key-field .value {
            background: #0f3460;
            padding: 1rem;
            border-radius: 6px;
            font-family: 'SF Mono', Monaco, Consolas, 'Courier New', monospace;
            font-size: 0.85rem;
            word-break: break-all;
            user-select: all;
            cursor: text;
        }
        .copy-btn {
            background: #0f3460;
            color: #4cc9f0;
            padding: 0.5rem 1rem;
            font-size: 0.8rem;
            margin-top: 0.5rem;
        }
        .copy-btn:hover { background: #1a4980; }
        .info { color: #888; font-size: 0.9rem; line-height: 1.6; }
        .info ul { margin-top: 0.5rem; padding-left: 1.5rem; }
        .info li { margin-bottom: 0.25rem; }
        .status { color: #4cc9f0; font-size: 0.9rem; margin-top: 0.5rem; min-height: 1.5em; }
        .status.error { color: #e94560; }
        .offline-badge {
            display: inline-block;
            background: #0f3460;
            color: #4cc9f0;
            padding: 0.25rem 0.75rem;
            border-radius: 20px;
            font-size: 0.8rem;
            margin-bottom: 1rem;
        }
        footer {
            margin-top: 3rem;
            text-align: center;
            color: #555;
            font-size: 0.85rem;
        }
        footer a { color: #4cc9f0; }
        .tabs { display: flex; margin-bottom: 1rem; }
        .tab {
            padding: 0.75rem 1.5rem;
            background: #0f3460;
            color: #888;
            border: none;
            cursor: pointer;
            font-size: 1rem;
        }
        .tab:first-child { border-radius: 8px 0 0 8px; }
        .tab:last-child { border-radius: 0 8px 8px 0; }
        .tab.active { background: #4cc9f0; color: #1a1a2e; }
        .tab-content { display: none; }
        .tab-content.active { display: block; }
    </style>
</head>
<body>
    <div class="container">
        <div class="offline-badge">Works Offline - Powered by WASM</div>
        <h1>Tunnels Protocol Key Generator</h1>
        <p class="subtitle">Generate and manage Ed25519 keypairs securely in your browser</p>

        <div class="card warning">
            <h3>Security Notice</h3>
            <p>This page generates cryptographic keys entirely in your browser using WebAssembly.
               No data is sent to any server. Your private key stays encrypted in WASM memory
               and is never exposed to JavaScript. For maximum security, save this page and use it offline.</p>
        </div>

        <div class="card">
            <div class="tabs">
                <button class="tab active" onclick="showTab('generate')">Generate New</button>
                <button class="tab" onclick="showTab('import')">Import Backup</button>
            </div>

            <div id="generate-tab" class="tab-content active">
                <div class="key-field">
                    <label>Password (for encrypted backup)</label>
                    <input type="password" id="password" placeholder="Enter a strong password">
                </div>
                <div class="key-field">
                    <label>Confirm Password</label>
                    <input type="password" id="password-confirm" placeholder="Confirm your password">
                </div>
                <button id="generate-btn" onclick="generateIdentity()">Generate Identity</button>
                <p class="status" id="generate-status"></p>
            </div>

            <div id="import-tab" class="tab-content">
                <div class="key-field">
                    <label>Select Backup File</label>
                    <input type="file" id="import-file" accept=".key,.tnls">
                </div>
                <div class="key-field">
                    <label>Backup Password</label>
                    <input type="password" id="import-password" placeholder="Enter backup password">
                </div>
                <button id="import-btn" onclick="importBackup()">Import Backup</button>
                <p class="status" id="import-status"></p>
            </div>
        </div>

        <div class="card key-display" id="key-display">
            <div class="key-field">
                <label>Identity Address</label>
                <div class="value" id="address"></div>
                <button class="copy-btn" onclick="copyToClipboard('address')">Copy Address</button>
            </div>

            <div class="key-field">
                <label>Public Key</label>
                <div class="value" id="public-key"></div>
                <button class="copy-btn" onclick="copyToClipboard('public-key')">Copy Public Key</button>
            </div>

            <div class="key-field">
                <button class="secondary-btn" id="download-btn" onclick="downloadBackup()">Download Encrypted Backup</button>
            </div>
        </div>

        <div class="card info">
            <h3>About Your Identity</h3>
            <ul>
                <li><strong>Address:</strong> Your unique 20-byte identifier on the Tunnels network.</li>
                <li><strong>Public Key:</strong> Share this to receive attestations and verify signatures.</li>
                <li><strong>Backup File:</strong> Encrypted with your password using Argon2id + AES-256-GCM.</li>
            </ul>
            <p style="margin-top: 1rem;">
                Your private key is encrypted inside the WASM module and never exposed to JavaScript.
                Only the encrypted backup file can be exported.
            </p>
        </div>

        <footer>
            <p>Tunnels Protocol | <a href="https://github.com/tunnelsprotocol" target="_blank">GitHub</a></p>
            <p style="margin-top: 0.5rem;">Built with the Tunnels SDK (Rust + WebAssembly)</p>
        </footer>
    </div>

    <script type="module">
// Inlined WASM binary (base64 encoded)
const WASM_BASE64 = "
HTMLHEAD

# Add the base64 WASM
echo -n "$WASM_BASE64" >> "$OUTPUT_FILE"

cat >> "$OUTPUT_FILE" << 'JSMID'
";

// Decode base64 to bytes
function base64ToBytes(base64) {
    const binary = atob(base64);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) {
        bytes[i] = binary.charCodeAt(i);
    }
    return bytes;
}

// ==== Inlined wasm-bindgen JS (modified) ====
JSMID

# Add the JS bindings (removing the export statements and modifying initialization)
echo "$JS_CONTENT" | \
    sed 's/^export class/class/' | \
    sed 's/^export function/function/' | \
    sed 's/^export { .*//' | \
    sed 's/async function __wbg_init(module_or_path) {/async function initWasm() {/' | \
    sed 's/if (module_or_path === undefined) {/const wasmBytes = base64ToBytes(WASM_BASE64); const module_or_path = wasmBytes; if (false) {/' \
    >> "$OUTPUT_FILE"

# Add the UI code
cat >> "$OUTPUT_FILE" << 'JSEND'

// ==== UI Code ====
let currentKeyHandle = null;
let currentPassword = null;

window.showTab = function(tab) {
    document.querySelectorAll('.tab').forEach(t => t.classList.remove('active'));
    document.querySelectorAll('.tab-content').forEach(t => t.classList.remove('active'));
    document.querySelector(`[onclick="showTab('${tab}')"]`).classList.add('active');
    document.getElementById(`${tab}-tab`).classList.add('active');
};

window.generateIdentity = async function() {
    const password = document.getElementById('password').value;
    const passwordConfirm = document.getElementById('password-confirm').value;
    const status = document.getElementById('generate-status');
    const btn = document.getElementById('generate-btn');

    status.classList.remove('error');

    if (!password) {
        status.textContent = 'Please enter a password';
        status.classList.add('error');
        return;
    }
    if (password !== passwordConfirm) {
        status.textContent = 'Passwords do not match';
        status.classList.add('error');
        return;
    }

    btn.disabled = true;
    status.textContent = 'Initializing WASM...';

    try {
        await initWasm();
        status.textContent = 'Generating secure keypair...';

        currentKeyHandle = generateKey();
        currentPassword = password;

        const address = currentKeyHandle.getAddress();
        const publicKey = currentKeyHandle.getPublicKey();

        document.getElementById('address').textContent = address;
        document.getElementById('public-key').textContent = publicKey;
        document.getElementById('key-display').classList.add('visible');

        status.textContent = 'Identity generated successfully!';
    } catch (e) {
        status.textContent = 'Error: ' + e.message;
        status.classList.add('error');
        console.error(e);
    }

    btn.disabled = false;
};

window.importBackup = async function() {
    const fileInput = document.getElementById('import-file');
    const password = document.getElementById('import-password').value;
    const status = document.getElementById('import-status');
    const btn = document.getElementById('import-btn');

    status.classList.remove('error');

    if (!fileInput.files.length) {
        status.textContent = 'Please select a backup file';
        status.classList.add('error');
        return;
    }
    if (!password) {
        status.textContent = 'Please enter the backup password';
        status.classList.add('error');
        return;
    }

    btn.disabled = true;
    status.textContent = 'Initializing WASM...';

    try {
        await initWasm();
        status.textContent = 'Decrypting backup...';

        const file = fileInput.files[0];
        const arrayBuffer = await file.arrayBuffer();
        const encrypted = new Uint8Array(arrayBuffer);

        currentKeyHandle = decryptKey(encrypted, password);
        currentPassword = password;

        const address = currentKeyHandle.getAddress();
        const publicKey = currentKeyHandle.getPublicKey();

        document.getElementById('address').textContent = address;
        document.getElementById('public-key').textContent = publicKey;
        document.getElementById('key-display').classList.add('visible');

        status.textContent = 'Backup imported successfully!';
    } catch (e) {
        status.textContent = 'Error: Wrong password or corrupted file';
        status.classList.add('error');
        console.error(e);
    }

    btn.disabled = false;
};

window.downloadBackup = function() {
    if (!currentKeyHandle || !currentPassword) {
        alert('No identity loaded');
        return;
    }

    try {
        const encrypted = currentKeyHandle.encryptForBackup(currentPassword);
        const blob = new Blob([encrypted], { type: 'application/octet-stream' });
        const url = URL.createObjectURL(blob);

        const a = document.createElement('a');
        a.href = url;
        const address = document.getElementById('address').textContent.slice(0, 8);
        a.download = `tunnels-${address}.key`;
        a.click();

        URL.revokeObjectURL(url);
    } catch (e) {
        alert('Error creating backup: ' + e.message);
        console.error(e);
    }
};

window.copyToClipboard = function(elementId) {
    const text = document.getElementById(elementId).textContent;
    navigator.clipboard.writeText(text).then(() => {
        const btn = event.target;
        const originalText = btn.textContent;
        btn.textContent = 'Copied!';
        setTimeout(() => btn.textContent = originalText, 1500);
    });
};
    </script>
</body>
</html>
JSEND

echo "Generated: $OUTPUT_FILE"
FILESIZE=$(stat -f%z "$OUTPUT_FILE" 2>/dev/null || stat -c%s "$OUTPUT_FILE" 2>/dev/null)
echo "File size: $(echo "scale=2; $FILESIZE / 1024" | bc) KB"
echo "SHA-256: $(shasum -a 256 "$OUTPUT_FILE" | cut -d' ' -f1)"
