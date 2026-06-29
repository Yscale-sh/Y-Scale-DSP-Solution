import { ref } from 'vue'

// Same-origin DSP API + live meter WebSocket.
// fetch URLs are relative so they work regardless of mount path.
export function useDspApi() {
  const meters = ref([]) // latest LINEAR peak per output channel
  const status = ref({ sample_rate: 0, n_in: 0, n_out: 0 })
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

  async function getConfig() {
    return jsonOrThrow(await fetch('api/config'))
  }

  async function putConfig(cfg) {
    return jsonOrThrow(
      await fetch('api/config', {
        method: 'PUT',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(cfg),
      }),
    )
  }

  async function postSource(spec) {
    return jsonOrThrow(
      await fetch('api/source', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(spec),
      }),
    )
  }

  async function playUrl(url) {
    return jsonOrThrow(
      await fetch('api/play', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ url }),
      }),
    )
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
      /* ignore; WS is the primary feed */
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
    refreshStatus()
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

  return { meters, status, wsState, getConfig, putConfig, postSource, playUrl, refreshStatus, start, stop }
}
