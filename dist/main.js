console.log('[DEBUG] main.js loading...');

// Tauri v2 uses window.__TAURI_INTERNALS__.invoke
let invoke;

// State management
let currentDeviceCode = null;

// DOM elements
const states = {
    detecting: document.getElementById('detecting-state'),
    code: document.getElementById('code-state'),
    authenticated: document.getElementById('authenticated-state'),
    settings: document.getElementById('settings-state'),
    error: document.getElementById('error-state'),
};

// Show only one state at a time
function showState(stateName) {
    Object.values(states).forEach(el => el.classList.add('hidden'));
    if (states[stateName]) {
        states[stateName].classList.remove('hidden');
    }
}

// Initialize app
async function init() {
    console.log('[DEBUG] init() called');

    // Wait for Tauri to be ready (Tauri v2 uses __TAURI_INTERNALS__)
    console.log('[DEBUG] Checking for Tauri API...');

    if (window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.invoke) {
        invoke = window.__TAURI_INTERNALS__.invoke;
        console.log('[DEBUG] Using __TAURI_INTERNALS__.invoke');
    } else if (window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke) {
        invoke = window.__TAURI__.core.invoke;
        console.log('[DEBUG] Using __TAURI__.core.invoke');
    } else {
        console.error('[DEBUG] Tauri not available yet, waiting...');
        setTimeout(init, 100);
        return;
    }

    console.log('[DEBUG] invoke function loaded:', typeof invoke);

    try {
        // Try to load saved folder path first
        const savedPath = await invoke('load_folder_path');
        console.log('[DEBUG] Saved folder path:', savedPath);

        if (savedPath) {
            // We have a saved folder, skip detection and go straight to auth
            console.log('[DEBUG] Using saved folder, starting device auth...');
            await startDeviceAuth();
            return;
        }

        // No saved path, show detecting state
        showState('detecting');
        console.log('[DEBUG] Showing detecting state');

        // Try to detect SC2 folder with timeout
        console.log('[DEBUG] Starting folder detection...');
        const folderPath = await detectWithTimeout();
        console.log('[DEBUG] Detection result:', folderPath);

        if (folderPath) {
            // Found folder, go straight to device auth
            console.log('[DEBUG] Found folder, starting device auth...');
            await startDeviceAuth();
        }
    } catch (error) {
        // If auto-detection fails, show option to pick manually
        console.error('[DEBUG] Detection error:', error);
        showManualPickerOption(error);
    }

    // Set up error retry button
    setupRetryButton();
}

// Set up retry button (can be called multiple times)
function setupRetryButton() {
    const retryBtn = document.getElementById('retry-btn');
    if (retryBtn) {
        // Remove old listener by cloning
        const newBtn = retryBtn.cloneNode(true);
        retryBtn.parentNode.replaceChild(newBtn, retryBtn);
        // Add fresh listener
        newBtn.addEventListener('click', () => {
            console.log('[DEBUG] Retry button clicked');
            // Reset to initial state and restart
            location.reload();
        });
    }
}

// Detect folder with 8 second timeout
async function detectWithTimeout() {
    const TIMEOUT_MS = 8000;

    console.log('[DEBUG] detectWithTimeout starting...');

    const detectionPromise = invoke('detect_replay_folder')
        .then(result => {
            console.log('[DEBUG] invoke SUCCESS:', result);
            return result;
        })
        .catch(err => {
            console.error('[DEBUG] invoke ERROR:', err);
            throw err;
        });

    const timeoutPromise = new Promise((_, reject) =>
        setTimeout(() => {
            console.log('[DEBUG] Timeout reached!');
            reject('timeout');
        }, TIMEOUT_MS)
    );

    return Promise.race([detectionPromise, timeoutPromise]);
}

// Show manual picker option
function showManualPickerOption(error) {
    const detectingState = document.getElementById('detecting-state');

    // Update the detecting state to show manual option
    const statusText = detectingState.querySelector('.status');
    statusText.textContent = 'Could not automatically find your SC2 replay folder.';

    const spinner = detectingState.querySelector('.spinner');
    spinner.style.display = 'none';

    // Add manual pick button if it doesn't exist
    if (!document.getElementById('manual-pick-btn')) {
        const button = document.createElement('button');
        button.id = 'manual-pick-btn';
        button.className = 'btn btn-primary';
        button.textContent = 'Choose Folder Manually';
        detectingState.appendChild(button);
        button.addEventListener('click', pickFolderManually);
    }
}

// Pick folder manually
async function pickFolderManually() {
    try {
        const folderPath = await invoke('pick_replay_folder_manual');

        // Go straight to device auth
        await startDeviceAuth();
    } catch (error) {
        if (error !== 'No folder selected') {
            showError(`Failed to select folder: ${error}`);
        }
        // If user cancelled, just stay on the manual picker screen
    }
}

// Start device code authentication flow
async function startDeviceAuth() {
    try {
        showState('detecting');

        // Request device code from server
        const response = await invoke('request_device_code');

        currentDeviceCode = response.device_code;

        // Display code to user
        document.getElementById('user-code').textContent = response.user_code;

        // Store verification URL for opening browser
        window.verificationUrl = response.verification_uri;

        showState('code');

        // Set up buttons - need to wait for DOM
        setTimeout(() => {
            // Set up "Open Browser" button
            const openBrowserBtn = document.getElementById('open-browser-btn');
            if (openBrowserBtn) {
                const newOpenBtn = openBrowserBtn.cloneNode(true);
                openBrowserBtn.parentNode.replaceChild(newOpenBtn, openBrowserBtn);
                newOpenBtn.addEventListener('click', openBrowser);
            }

            // Set up "Check Authorization" button
            const checkBtn = document.getElementById('check-auth-btn');
            if (checkBtn) {
                const newCheckBtn = checkBtn.cloneNode(true);
                checkBtn.parentNode.replaceChild(newCheckBtn, checkBtn);
                newCheckBtn.addEventListener('click', () => checkAuthorization(response.device_code));
            }
        }, 100);
    } catch (error) {
        showError(`Failed to request device code: ${error}`);
    }
}

// Check authorization status (called when user clicks button)
async function checkAuthorization(deviceCode) {
    const checkBtn = document.getElementById('check-auth-btn');
    const originalText = checkBtn.textContent;

    try {
        checkBtn.disabled = true;
        checkBtn.textContent = 'Checking...';

        const response = await invoke('poll_device_authorization', { deviceCode });

        // Success! Show authenticated state
        document.getElementById('username').textContent = response.user.username;
        if (response.user.avatar_url) {
            document.getElementById('user-avatar').src = response.user.avatar_url;
        }

        showState('authenticated');

        // Store tokens securely
        await invoke('save_auth_tokens', {
            accessToken: response.access_token,
            refreshToken: response.refresh_token || null,
            expiresAt: response.expires_at || null
        });

        // Set up settings button
        setTimeout(() => {
            const settingsBtn = document.getElementById('settings-btn');
            if (settingsBtn) {
                const newSettingsBtn = settingsBtn.cloneNode(true);
                settingsBtn.parentNode.replaceChild(newSettingsBtn, settingsBtn);
                newSettingsBtn.addEventListener('click', openSettings);
            }
        }, 100);

    } catch (error) {
        checkBtn.disabled = false;
        checkBtn.textContent = originalText;

        // Handle different error types
        if (error === 'pending') {
            // Still waiting - show a message
            const statusEl = document.querySelector('#code-state .status');
            if (statusEl) {
                statusEl.textContent = 'Not authorized yet. Please complete authorization on the website.';
                setTimeout(() => {
                    statusEl.textContent = 'After completing activation, click below:';
                }, 3000);
            }
            return;
        }

        if (error === 'expired') {
            showError('Device code expired. Please try again.');
            return;
        }

        if (error === 'denied') {
            showError('Authorization denied. Please try again.');
            return;
        }

        // Unknown error
        console.error('Authorization error:', error);
        showError(`Authorization failed: ${error}`);
    }
}

// Open browser to activation page
async function openBrowser() {
    try {
        if (!window.verificationUrl) {
            console.error('No verification URL available');
            return;
        }
        await invoke('open_browser', { url: window.verificationUrl });
    } catch (error) {
        console.error('Failed to open browser:', error);
    }
}

// Show error state
function showError(message) {
    console.log('[DEBUG] Showing error:', message);
    document.getElementById('error-message').textContent = message;
    showState('error');
    // Set up retry button whenever we show an error
    setupRetryButton();
}

// Settings functions
async function openSettings() {
    console.log('[DEBUG] Opening settings');
    showState('settings');

    // Load current autostart setting
    try {
        const enabled = await invoke('get_autostart_enabled');
        document.getElementById('autostart-toggle').checked = enabled;
    } catch (error) {
        console.error('Failed to load autostart setting:', error);
    }

    // Set up event listeners
    setTimeout(() => {
        // Autostart toggle
        const autostartToggle = document.getElementById('autostart-toggle');
        if (autostartToggle) {
            autostartToggle.addEventListener('change', async (e) => {
                try {
                    await invoke('set_autostart_enabled', { enabled: e.target.checked });
                } catch (error) {
                    console.error('Failed to set autostart:', error);
                    e.target.checked = !e.target.checked; // Revert on error
                }
            });
        }

        // Logout button
        const logoutBtn = document.getElementById('logout-btn');
        if (logoutBtn) {
            const newLogoutBtn = logoutBtn.cloneNode(true);
            logoutBtn.parentNode.replaceChild(newLogoutBtn, logoutBtn);
            newLogoutBtn.addEventListener('click', handleLogout);
        }

        // Back button
        const backBtn = document.getElementById('back-from-settings-btn');
        if (backBtn) {
            const newBackBtn = backBtn.cloneNode(true);
            backBtn.parentNode.replaceChild(newBackBtn, backBtn);
            newBackBtn.addEventListener('click', () => {
                showState('authenticated');
            });
        }
    }, 100);
}

async function handleLogout() {
    console.log('[DEBUG] Logging out');

    if (!confirm('Are you sure you want to logout?')) {
        return;
    }

    try {
        // Clear tokens
        await invoke('clear_auth_tokens');

        // Restart the app
        location.reload();
    } catch (error) {
        console.error('Failed to logout:', error);
        showError(`Failed to logout: ${error}`);
    }
}

// Start the app
console.log('[DEBUG] Setting up DOMContentLoaded listener');
window.addEventListener('DOMContentLoaded', () => {
    console.log('[DEBUG] DOMContentLoaded fired!');
    init();
});
