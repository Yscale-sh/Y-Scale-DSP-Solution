import { ref } from 'vue'

// Same-origin DSP + streamer API and the live WebSocket (meters + now-playing +
// volume). fetch URLs are relative so they work regardless of mount path.
export function useDspApi() {
  const meters = ref([]) // latest LINEAR peak per output channel
  const gr = ref(0) // safety-limiter gain reduction (dB, >= 0)
  const spectrum = ref([]) // RTA: per-band magnitude dBFS (30 ISO bands)
  const status = ref({ sample_rate: 0, n_in: 0, n_out: 0 })
  const now = ref({ state: 'stopped', title: '', artist: '', album: '', art_url: '', position: 0, duration: 0, source: 'idle' })
  const volume = ref({ pct: 45, db: -33, muted: false })
  const wsState = ref('connecting') // 'connecting' | 'live' | 'down'

  let ws = null
  let reconnectTimer = null
  let alive = true

  async function jsonOrThrow(res) {
    let data = null
    try {
      data = await res.json()
    } catch {
      /* no body */
    }
    if (!res.ok) {
      const msg = (data && data.error) || `${res.status} ${res.statusText}`
      throw new Error(msg)
    }
    return data
  }

  const getJson = (path) => fetch(path).then(jsonOrThrow)
  const send = (path, method, body) =>
    fetch(path, {
      method,
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(body),
    }).then(jsonOrThrow)

  // ── DSP graph ──────────────────────────────────────────────────────────────
  const getConfig = () => getJson('api/config')
  const putConfig = (cfg) => send('api/config', 'PUT', cfg)

  // ── Sources / transport ─────────────────────────────────────────────────────
  const postSource = (spec) => send('api/source', 'POST', spec)
  const playUrl = (url, meta = {}) => send('api/play', 'POST', { url, ...meta })
  const pause = (paused) => send('api/pause', 'POST', { paused })
  const stopPlayback = () => send('api/stop', 'POST', {})
  const seek = (position) => send('api/seek', 'POST', { position })

  // ── Presets / scenes ─────────────────────────────────────────────────────────
  const getPresets = () => getJson('api/presets')
  const savePreset = (name) => send('api/presets/save', 'POST', { name })
  const loadPreset = (name) => send('api/presets/load', 'POST', { name })
  const deletePreset = (name) => send('api/presets/delete', 'POST', { name })

  // ── FIR convolution / room correction ────────────────────────────────────────
  const getFirs = () => getJson('api/firs')
  const uploadFir = (name, buffer) =>
    fetch(`api/firs/upload?name=${encodeURIComponent(name)}`, { method: 'POST', body: buffer }).then(jsonOrThrow)
  const deleteFir = (name) => send('api/firs/delete', 'POST', { name })

  // ── Master volume ────────────────────────────────────────────────────────────
  async function setVolume(body) {
    const v = await send('api/volume', 'PUT', body)
    if (v) volume.value = v
    return v
  }

  async function refreshNow() {
    try {
      const d = await getJson('api/now')
      if (d.now) now.value = d.now
      if (d.volume) volume.value = d.volume
      if (Array.isArray(d.meters)) meters.value = d.meters
      if (typeof d.gr === 'number') gr.value = d.gr
      if (Array.isArray(d.spectrum)) spectrum.value = d.spectrum
      if (typeof d.sample_rate === 'number') {
        status.value = { sample_rate: d.sample_rate, n_in: d.n_in, n_out: d.n_out }
      }
    } catch {
      /* ignore; WS is the primary feed */
    }
  }

  async function refreshStatus() {
    try {
      const res = await fetch('api/status')
      if (res.ok) {
        const d = await res.json()
        status.value = d
        if (Array.isArray(d.meters)) meters.value = d.meters
      }
    } catch {
      /* ignore */
    }
  }

  function connectWs() {
    if (!alive) return
    const proto = location.protocol === 'https:' ? 'wss' : 'ws'
    let socket
    try {
      socket = new WebSocket(`${proto}://${location.host}/ws`)
    } catch {
      scheduleReconnect()
      return
    }
    ws = socket
    socket.onopen = () => {
      if (ws === socket) wsState.value = 'live'
    }
    socket.onmessage = (ev) => {
      try {
        const msg = JSON.parse(ev.data)
        if (msg && Array.isArray(msg.meters)) meters.value = msg.meters
        if (msg && typeof msg.gr === 'number') gr.value = msg.gr
        if (msg && Array.isArray(msg.spectrum)) spectrum.value = msg.spectrum
        if (msg && msg.now) now.value = msg.now
        if (msg && msg.volume) volume.value = msg.volume
      } catch {
        /* ignore malformed frame */
      }
    }
    socket.onclose = () => {
      if (ws === socket) {
        wsState.value = 'down'
        scheduleReconnect()
      }
    }
    socket.onerror = () => {
      try {
        socket.close()
      } catch {
        /* noop */
      }
    }
  }

  function scheduleReconnect() {
    if (!alive) return
    clearTimeout(reconnectTimer)
    reconnectTimer = setTimeout(() => {
      wsState.value = 'connecting'
      connectWs()
    }, 1500)
  }

  function start() {
    alive = true
    refreshNow()
    connectWs()
  }

  function stop() {
    alive = false
    clearTimeout(reconnectTimer)
    try {
      ws && ws.close()
    } catch {
      /* noop */
    }
  }

  return {
    meters, gr, spectrum, status, now, volume, wsState,
    getConfig, putConfig,
    postSource, playUrl, pause, stopPlayback, seek,
    setVolume, refreshNow, refreshStatus,
    getPresets, savePreset, loadPreset, deletePreset,
    getFirs, uploadFir, deleteFir,
    start, stop,
  }
}
