<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { onMount, onDestroy } from 'svelte';

  // Types matching backend events
  interface TranscriptionUpdate {
    committed: string;
    tentative?: string;
    timestamp: number;
    latency_ms?: number;
  }

  interface SessionStatus {
    state: string;
    timestamp: number;
    metrics?: {
      rtf: number;
      tokens_per_sec: number;
      audio_duration_ms: number;
    };
  }

  // State with Svelte 5 runes
  let committed = $state('');
  let tentative = $state('');
  let status = $state<'idle' | 'listening' | 'processing' | 'error'>('idle');
  let statusMessage = $state('');
  let isScrollPaused = $state(false);
  let autoScroll = $state(true);
  let audioDevices = $state<string[]>([]);
  let selectedDevice = $state('');
  let targetDelay = $state(480);
  let vadSensitivity = $state(0.5);

  // Model download / onboarding state
  let modelReady = $state(false);
  let isDownloading = $state(false);
  let downloadProgress = $state({
    downloaded: 0,
    total: 0,
    percentage: 0,
    speed: 0, // bytes/sec
    eta: 0,   // seconds
  });

  // For editing
  let isEditing = $state(false);
  let editBuffer = $state('');

  let transcriptionContainer: HTMLDivElement | null = $state(null);
  let unlistenFns: UnlistenFn[] = [];

  // Derived for display
  let displayText = $derived(committed + (tentative ? ` <span class="tentative">${tentative}</span>` : ''));

  // Metrics
  let lastRtf = $state(0);
  let lastLatency = $state<number | null>(null);

  async function loadDevices() {
    try {
      audioDevices = await invoke<string[]>('list_input_devices');
      if (audioDevices.length > 0 && !selectedDevice) {
        selectedDevice = audioDevices[0];
      }
    } catch (e) {
      console.error('Failed to load devices', e);
    }
  }

  async function startRecording(ptt = false) {
    try {
      await invoke('start_recording', { pushToTalk: ptt });
      status = 'listening';
      statusMessage = ptt ? 'Push-to-talk active' : 'Listening...';
      committed = '';
      tentative = '';
      if (autoScroll && transcriptionContainer) {
        transcriptionContainer.scrollTop = transcriptionContainer.scrollHeight;
      }
    } catch (e: any) {
      status = 'error';
      statusMessage = e?.toString() || 'Failed to start';
    }
  }

  async function stopRecording() {
    try {
      await invoke('stop_recording');
      status = 'processing';
      statusMessage = 'Finalizing...';
    } catch (e: any) {
      status = 'error';
      statusMessage = e?.toString() || 'Failed to stop';
    }
  }

  async function cancelRecording() {
    try {
      await invoke('cancel_recording');
      status = 'idle';
      statusMessage = '';
      committed = '';
      tentative = '';
    } catch (e: any) {
      status = 'error';
      statusMessage = e?.toString();
    }
  }

  function toggleRecording() {
    if (status === 'listening' || status === 'processing') {
      stopRecording();
    } else {
      startRecording(false);
    }
  }

  async function updateSettings() {
    // In a full app these would be sent to backend via commands
    // For now just log; extend with commands like change_delay etc.
    console.log('Settings updated (demo):', { targetDelay, vadSensitivity, selectedDevice });
  }

  async function startModelDownload() {
    isDownloading = true;
    downloadProgress = { downloaded: 0, total: 0, percentage: 0, speed: 0, eta: 0 };
    try {
      await invoke('ensure_model_downloaded');
      // Progress comes via events; on complete modelReady will be set
    } catch (e: any) {
      isDownloading = false;
      status = 'error';
      statusMessage = 'Download failed: ' + (e?.toString() || '');
    }
  }

  async function pauseDownload() {
    await invoke('pause_model_download');
  }

  async function resumeDownload() {
    await invoke('resume_model_download');
  }

  async function cancelDownload() {
    await invoke('cancel_model_download');
    isDownloading = false;
  }

  async function deleteCurrentModel() {
    try {
      await invoke('delete_model');
      modelReady = false;
      committed = '';
      tentative = '';
      statusMessage = 'Model deleted. Restart download.';
    } catch (e: any) {
      statusMessage = 'Delete failed: ' + e;
    }
  }

  function startInlineEdit() {
    isEditing = true;
    editBuffer = committed;
  }

  function saveEdit() {
    committed = editBuffer.trim();
    isEditing = false;
    // TODO: send edit to backend if supported (e.g. via a command)
  }

  function cancelEdit() {
    isEditing = false;
  }

  function toggleScrollPause() {
    isScrollPaused = !isScrollPaused;
    autoScroll = !isScrollPaused;
  }

  function scrollToBottom(force = false) {
    if (!transcriptionContainer) return;
    if (force || (autoScroll && !isScrollPaused)) {
      transcriptionContainer.scrollTop = transcriptionContainer.scrollHeight;
    }
  }

  // Listen to Tauri events with proper cleanup
  async function setupListeners() {
    const unlistenUpdate = await listen<TranscriptionUpdate>('transcription-update', (event) => {
      const payload = event.payload;
      committed = payload.committed || committed;
      tentative = payload.tentative || '';
      lastLatency = payload.latency_ms ?? lastLatency;

      // Auto-scroll on update
      requestAnimationFrame(() => scrollToBottom());
    });

    const unlistenStatus = await listen<SessionStatus>('session-status', (event) => {
      const p = event.payload;
      if (p.state) {
        status = p.state as any;
        if (p.state === 'idle') {
          tentative = '';
          statusMessage = '';
        } else if (p.state === 'listening') {
          statusMessage = 'Listening for speech...';
        }
      }
      if (p.metrics) {
        lastRtf = p.metrics.rtf;
      }
    });

    const unlistenFinal = await listen<{ text: string }>('transcription-final', (event) => {
      committed = event.payload.text || committed;
      tentative = '';
      status = 'idle';
      statusMessage = 'Transcription complete';
      scrollToBottom(true);
    });

    const unlistenError = await listen<{ message: string }>('voxly-error', (event) => {
      status = 'error';
      statusMessage = event.payload.message;
    });

    // Model download events for onboarding screen
    const unlistenModelProgress = await listen<any>('model-download-progress', (event) => {
      const p = event.payload;
      downloadProgress = {
        downloaded: p.downloaded_bytes || p.downloaded || 0,
        total: p.total_bytes || p.total || 0,
        percentage: p.percentage || 0,
        speed: p.speed_bps || 0,
        eta: p.eta_seconds || 0,
      };
      isDownloading = true;
    });

    const unlistenModelState = await listen<any>('model-state-changed', (event) => {
      const p = event.payload;
      if (p.event_type === 'download_completed' || p.event_type === 'ready') {
        isDownloading = false;
        modelReady = true;
        statusMessage = 'Model ready';
      } else if (p.event_type === 'download_started') {
        isDownloading = true;
        modelReady = false;
      }
    });

    unlistenFns = [unlistenUpdate, unlistenStatus, unlistenFinal, unlistenError, unlistenModelProgress, unlistenModelState];
  }

  onMount(async () => {
    await loadDevices();
    await setupListeners();

    // Check if model is present (drives onboarding vs main UI)
    try {
      modelReady = await invoke<boolean>('is_model_downloaded');
    } catch (_) {
      modelReady = false;
    }

    // If not ready, optionally auto-trigger (or let user click Download)
    if (!modelReady) {
      // UI will show onboarding screen
    }
  });

  onDestroy(() => {
    unlistenFns.forEach((fn) => fn());
  });

  // Keyboard support (local, global via Tauri)
  function handleKeydown(e: KeyboardEvent) {
    if (e.key === ' ' && (document.activeElement?.tagName === 'BODY' || document.activeElement === transcriptionContainer)) {
      e.preventDefault();
      toggleRecording();
    }
    if (e.key.toLowerCase() === 'escape') {
      cancelRecording();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if !modelReady}
  <div class="h-screen flex items-center justify-center bg-slate-950 p-8">
    <div class="max-w-lg w-full bg-slate-900 border border-slate-800 rounded-3xl p-8">
      <h1 class="text-2xl font-semibold mb-2">Download the model</h1>
      <p class="text-slate-400 mb-6">Voxly needs the Voxtral Q4 GGUF model (~2.5 GB) for fully local inference.</p>

      {#if isDownloading}
        <div class="mb-4">
          <div class="h-2 bg-slate-800 rounded-full overflow-hidden mb-2">
            <div class="h-2 bg-sky-500 transition-all" style="width: {downloadProgress.percentage}%"></div>
          </div>
          <div class="flex justify-between text-xs text-slate-400">
            <span>{downloadProgress.percentage.toFixed(1)}%</span>
            <span>{(downloadProgress.speed/1e6).toFixed(1)} MB/s • ETA {Math.round(downloadProgress.eta/60)}m</span>
          </div>
          <div class="text-[10px] text-slate-500 mt-1">Downloaded {(downloadProgress.downloaded/1e9).toFixed(2)} GB / {(downloadProgress.total/1e9).toFixed(2)} GB</div>
        </div>
        <div class="flex gap-2">
          <button onclick={pauseDownload} class="flex-1 py-2 bg-slate-800 rounded-xl">Pause</button>
          <button onclick={resumeDownload} class="flex-1 py-2 bg-slate-800 rounded-xl">Resume</button>
          <button onclick={cancelDownload} class="flex-1 py-2 bg-red-950 rounded-xl">Cancel</button>
        </div>
      {:else}
        <button onclick={startModelDownload} class="w-full py-3 bg-sky-600 hover:bg-sky-500 rounded-2xl font-medium">Start Download</button>
      {/if}
      <button onclick={deleteCurrentModel} class="mt-3 text-xs text-red-400">Delete cached model</button>
    </div>
  </div>
{:else}
<div class="flex flex-col h-screen max-w-5xl mx-auto p-4 gap-4">
  <!-- Header -->
  <div class="flex items-center justify-between">
    <div class="flex items-center gap-3">
      <div class="text-3xl font-semibold tracking-tight">Voxly</div>
      <div class="text-xs px-2 py-0.5 rounded bg-slate-800 text-slate-400">local • private • realtime</div>
    </div>

    <div class="flex items-center gap-2 text-sm">
      <div class="status-pill {status === 'listening' ? 'bg-emerald-500/20 text-emerald-400' : status === 'processing' ? 'bg-amber-500/20 text-amber-400' : status === 'error' ? 'bg-red-500/20 text-red-400' : 'bg-slate-700 text-slate-400'}">
        <span class="w-2 h-2 rounded-full {status === 'listening' ? 'bg-emerald-400 animate-pulse' : status === 'processing' ? 'bg-amber-400' : status === 'error' ? 'bg-red-400' : 'bg-slate-400'}"></span>
        {status.charAt(0).toUpperCase() + status.slice(1)}
      </div>
      {#if lastRtf > 0}
        <div class="text-xs text-slate-500">RTF {lastRtf.toFixed(2)}</div>
      {/if}
      {#if lastLatency}
        <div class="text-xs text-slate-500">{lastLatency}ms</div>
      {/if}
    </div>
  </div>

  <!-- Transcription Area -->
  <div class="flex-1 flex flex-col min-h-0 border border-slate-800 rounded-2xl overflow-hidden bg-slate-900">
    <div class="flex items-center justify-between px-4 py-2 border-b border-slate-800 bg-slate-950/60">
      <div class="text-sm font-medium text-slate-400">Live Transcription</div>
      <div class="flex items-center gap-2">
        <button
          onclick={toggleScrollPause}
          class="text-xs px-3 py-1 rounded-lg border border-slate-700 hover:bg-slate-800 transition"
        >
          {isScrollPaused ? 'Resume auto-scroll' : 'Pause auto-scroll'}
        </button>
        <button
          onclick={() => { committed = ''; tentative = ''; }}
          class="text-xs px-3 py-1 rounded-lg border border-slate-700 hover:bg-slate-800 transition"
        >
          Clear
        </button>
      </div>
    </div>

    <div
      bind:this={transcriptionContainer}
      class="flex-1 overflow-auto p-6 text-lg leading-relaxed scroll-container"
      onscroll={() => {
        if (transcriptionContainer && autoScroll) {
          const nearBottom = transcriptionContainer.scrollHeight - transcriptionContainer.scrollTop - transcriptionContainer.clientHeight < 80;
          if (!nearBottom) isScrollPaused = true;
        }
      }}
    >
      {#if isEditing}
        <textarea
          bind:value={editBuffer}
          class="w-full min-h-[200px] bg-slate-950 border border-slate-700 rounded-xl p-4 font-mono text-base focus:outline-none focus:border-slate-500"
          onblur={saveEdit}
          onkeydown={(e) => { if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) { e.preventDefault(); saveEdit(); } }}
        ></textarea>
        <div class="text-xs text-slate-500 mt-1">Cmd/Ctrl+Enter to save • Esc to cancel</div>
      {:else}
        <div class="transcription-area text-[15px]">
          <span class="committed">{committed}</span>
          {#if tentative}
            <span class="tentative ml-1">{tentative}</span>
          {/if}
          {#if !committed && !tentative}
            <span class="text-slate-600 italic select-none">Waiting for speech... Press Start or use hotkey.</span>
          {/if}
        </div>
      {/if}
    </div>

    {#if committed && !isEditing}
      <div class="border-t border-slate-800 px-4 py-2 flex justify-end">
        <button
          onclick={startInlineEdit}
          class="text-xs text-slate-400 hover:text-slate-200 px-2 py-1 rounded hover:bg-slate-800"
        >
          Edit committed text
        </button>
      </div>
    {/if}
  </div>

  <!-- Controls -->
  <div class="flex items-center justify-between gap-3">
    <div class="flex gap-3">
      <button
        onclick={toggleRecording}
        class="control-btn text-xl min-w-[160px] {status === 'listening' || status === 'processing' ? 'bg-red-600 hover:bg-red-500' : 'bg-emerald-600 hover:bg-emerald-500'}"
      >
        {status === 'listening' || status === 'processing' ? '■ Stop' : '▶ Start'}
      </button>

      <button
        onclick={() => startRecording(true)}
        disabled={status === 'listening'}
        class="control-btn bg-slate-800 hover:bg-slate-700 border border-slate-700"
      >
        Hold to Talk (PTT)
      </button>

      <button
        onclick={cancelRecording}
        class="control-btn bg-slate-800 hover:bg-slate-700 border border-slate-700"
      >
        Cancel
      </button>
    </div>

    <div class="text-xs text-slate-500">
      Space: toggle • Esc: cancel • Global hotkeys supported via Tauri
    </div>
  </div>

  <!-- Settings -->
  <details class="border border-slate-800 rounded-2xl p-4 bg-slate-900">
    <summary class="cursor-pointer text-sm font-medium select-none">Settings &amp; Devices</summary>
    <div class="mt-4 grid grid-cols-1 md:grid-cols-2 gap-4 text-sm">
      <div>
        <label class="block text-xs text-slate-400 mb-1">Audio Input Device</label>
        <select bind:value={selectedDevice} onchange={updateSettings} class="w-full bg-slate-950 border border-slate-700 rounded-lg p-2">
          {#each audioDevices as dev}
            <option value={dev}>{dev}</option>
          {/each}
          {#if audioDevices.length === 0}
            <option>Default (no devices listed)</option>
          {/if}
        </select>
      </div>

      <div>
        <label for="delay" class="block text-xs text-slate-400 mb-1">Target Delay: {targetDelay} ms</label>
        <input id="delay" type="range" min="80" max="2400" step="20" bind:value={targetDelay} oninput={updateSettings} class="w-full accent-sky-500" />
        <div class="flex justify-between text-[10px] text-slate-500"><div>Low latency</div><div>High accuracy</div></div>
      </div>

      <div>
        <label for="vad-sens" class="block text-xs text-slate-400 mb-1">VAD Sensitivity: {vadSensitivity.toFixed(1)}</label>
        <input id="vad-sens" type="range" min="0" max="1" step="0.05" bind:value={vadSensitivity} oninput={updateSettings} class="w-full accent-sky-500" />
      </div>

      <div class="text-xs text-slate-400 self-end">
        Changes take effect on next recording. Full backend sync coming soon.
      </div>
    </div>
  </details>

  <div class="text-[10px] text-center text-slate-600">
    Svelte 5 runes • fine-grained updates • Tauri v2 events
  </div>
</div>
{/if}

<style>
  /* Additional scoped styles if needed */
</style>