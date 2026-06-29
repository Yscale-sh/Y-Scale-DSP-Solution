<script setup>
import { reactive, ref, computed, watch, onMounted, onBeforeUnmount } from 'vue'
import { useDspApi } from './composables/useDspApi.js'
import { uid } from './lib/util.js'
import { clamp } from './lib/dsp.js'
import SourceBar from './components/SourceBar.vue'
import MasterMeters from './components/MasterMeters.vue'
import RoutingPanel from './components/RoutingPanel.vue'
import ChannelStrip from './components/ChannelStrip.vue'
import Toast from './components/Toast.vue'

const api = useDspApi()
const { meters, status, wsState } = api

const ACCENTS = ['var(--color-signal)', 'var(--color-violet)', 'var(--color-cool)', 'var(--color-amber)']
const defaultName = (i) => ['Left', 'Right'][i] ?? `Channel ${i + 1}`

const loaded = ref(false)
const loadError = ref('')
let ready = false

const cfg = reactive({
  sample_rate: 48000,
  device: '',
  format: 's32_le',
  period_frames: 1024,
  buffer_frames: 4096,
  dither: false,
  routing: { preset: 'stereo', matrix: null },
  channels: [],
})

// ── model factories ─────────────────────────────────────────────────────────
function makeBand(b = {}) {
  return {
    _id: uid('band'),
    kind: b.kind ?? 'peaking',
    freq: b.freq ?? 1000,
    q: b.q ?? 0.707,
    gain_db: b.gain_db ?? 0,
  }
}
function padGraphic(arr) {
  const out = new Array(30).fill(0)
  for (let i = 0; i < Math.min(30, arr.length); i++) out[i] = arr[i] ?? 0
  return out
}
function makeChannel(src, i) {
  return {
    _id: uid('ch'),
    name: src?.name ?? defaultName(i),
    gain_db: src?.gain_db ?? 0,
    delay_ms: src?.delay_ms ?? 0,
    delay_cm: src?.delay_cm ?? 0,
    invert: !!src?.invert,
    mute: !!src?.mute,
    eq: Array.isArray(src?.eq) ? src.eq.map(makeBand) : [],
    graphic_eq: Array.isArray(src?.graphic_eq) ? padGraphic(src.graphic_eq) : null,
    crossover: src?.crossover
      ? { kind: src.crossover.kind, role: src.crossover.role, freq: src.crossover.freq, order: src.crossover.order }
      : null,
  }
}

// ── serialize for PUT (only server-allowed fields; values clamped) ───────────
const r = (v, d = 3) => {
  const n = Number(v)
  return Number.isFinite(n) ? +n.toFixed(d) : 0
}
function buildPayload() {
  return {
    sample_rate: cfg.sample_rate,
    device: cfg.device,
    format: cfg.format,
    period_frames: cfg.period_frames,
    buffer_frames: cfg.buffer_frames,
    dither: !!cfg.dither,
    routing: { preset: cfg.routing.preset, matrix: cfg.routing.matrix ?? null },
    channel: cfg.channels.map((ch) => ({
      name: ch.name ?? null,
      gain_db: r(clamp(ch.gain_db, -60, 12), 2),
      delay_ms: r(Math.max(0, ch.delay_ms), 3),
      delay_cm: r(Math.max(0, ch.delay_cm), 3),
      invert: !!ch.invert,
      mute: !!ch.mute,
      eq: ch.eq.map((b) => ({
        kind: b.kind,
        freq: r(clamp(b.freq, 10, 20000), 2),
        q: r(clamp(b.q, 0.05, 10), 3),
        gain_db: r(clamp(b.gain_db, -24, 24), 2),
      })),
      graphic_eq: ch.graphic_eq ? ch.graphic_eq.map((v) => r(clamp(v, -12, 12), 2)) : null,
      crossover: ch.crossover
        ? {
            kind: ch.crossover.kind,
            role: ch.crossover.role,
            freq: r(clamp(ch.crossover.freq, 10, 20000), 2),
            order: clamp(Math.round(ch.crossover.order), 1, 4),
          }
        : null,
    })),
  }
}

// ── live apply (debounced) ───────────────────────────────────────────────────
const toast = ref(null)
let toastTimer
function showToast(msg, kind = 'ok') {
  toast.value = { msg, kind }
  clearTimeout(toastTimer)
  toastTimer = setTimeout(() => (toast.value = null), kind === 'error' ? 4200 : 1200)
}

let putTimer
let putInFlight = false
async function doPut() {
  if (putInFlight) {
    schedulePut()
    return
  }
  putInFlight = true
  try {
    await api.putConfig(buildPayload())
    showToast('Applied', 'ok')
  } catch (e) {
    showToast(e.message || 'Apply failed', 'error')
  } finally {
    putInFlight = false
  }
}
function schedulePut() {
  clearTimeout(putTimer)
  putTimer = setTimeout(doPut, 150)
}

watch(buildPayload, () => {
  if (ready) schedulePut()
}, { deep: true })

// ── source transport ─────────────────────────────────────────────────────────
const nowPlaying = ref('')
function describe(spec, label) {
  switch (spec.kind) {
    case 'sine': return `${label} · ${spec.freq} Hz`
    case 'sweep': return `${label} · ${spec.f1}–${spec.f2} Hz`
    case 'impulse': return `${label} · ${spec.period_ms} ms`
    case 'file': return `${label} · ${(spec.path || '').split('/').pop()}`
    default: return label
  }
}
async function onPlay({ spec, label }) {
  try {
    await api.postSource(spec)
    nowPlaying.value = describe(spec, label)
    showToast(`${label} playing`, 'ok')
  } catch (e) {
    showToast(e.message || 'Source failed', 'error')
  }
}
async function onStop() {
  try {
    await api.postSource({ kind: 'silence' })
    nowPlaying.value = ''
    showToast('Stopped', 'ok')
  } catch (e) {
    showToast(e.message || 'Stop failed', 'error')
  }
}

// ── derived ───────────────────────────────────────────────────────────────────
const fs = computed(() => status.value.sample_rate || cfg.sample_rate || 48000)
const meterChannels = computed(() =>
  cfg.channels.map((c, i) => ({ name: c.name || defaultName(i), accent: ACCENTS[i % ACCENTS.length] })),
)
const nOut = computed(() => status.value.n_out || cfg.channels.length || 2)

// ── boot ──────────────────────────────────────────────────────────────────────
async function hydrate() {
  const fetched = await api.getConfig()
  cfg.sample_rate = fetched.sample_rate ?? 48000
  cfg.device = fetched.device ?? ''
  cfg.format = fetched.format ?? 's32_le'
  cfg.period_frames = fetched.period_frames ?? 1024
  cfg.buffer_frames = fetched.buffer_frames ?? 4096
  cfg.dither = !!fetched.dither
  cfg.routing.preset = fetched.routing?.preset ?? 'stereo'
  cfg.routing.matrix = fetched.routing?.matrix ?? null

  const chans = Array.isArray(fetched.channel) ? fetched.channel : []
  if (chans.length === 0) {
    const n = status.value.n_out || 2
    cfg.channels = Array.from({ length: n }, (_, i) => makeChannel(null, i))
  } else {
    cfg.channels = chans.map((c, i) => makeChannel(c, i))
  }
}

onMounted(async () => {
  await api.refreshStatus()
  try {
    await hydrate()
    loaded.value = true
    ready = true
    // Always PUT explicit channels once on load.
    doPut()
  } catch (e) {
    loadError.value = e.message || 'Could not reach the DSP server.'
  }
  api.start()
})

onBeforeUnmount(() => api.stop())
</script>

<template>
  <div class="min-h-dvh px-4 sm:px-6 py-5 md:py-7 max-w-[1480px] mx-auto">
    <!-- header -->
    <header class="flex items-end justify-between gap-4 mb-6 rise" style="animation-delay: 0ms">
      <div class="flex items-center gap-3.5">
        <div
          class="grid place-items-center w-11 h-11 rounded-xl flex-none"
          style="
            background: linear-gradient(160deg, color-mix(in oklab, var(--color-signal) 28%, #0b1014), #0b1014);
            border: 1px solid color-mix(in oklab, var(--color-signal) 45%, transparent);
            box-shadow: 0 0 24px -6px color-mix(in oklab, var(--color-signal) 70%, transparent);
          "
        >
          <!-- waveform mark -->
          <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="var(--color-signal)" stroke-width="2" stroke-linecap="round">
            <path d="M2 12h3l2-7 4 14 3-9 2 4h6" />
          </svg>
        </div>
        <div class="leading-none">
          <h1 class="font-display font-bold text-2xl sm:text-[28px] tracking-tight text-ink">
            Y<span class="text-signal">//</span>SCALE
            <span class="text-dim font-medium">DSP</span>
          </h1>
          <p class="eyebrow mt-1.5">Loudspeaker Tuning · Live Controller</p>
        </div>
      </div>

      <div class="hidden sm:flex items-center gap-2.5 readout text-[11px]">
        <span class="px-3 py-1.5 rounded-lg border border-hair bg-[rgba(255,255,255,0.02)] text-dim">
          {{ (fs / 1000).toFixed(1) }} kHz
        </span>
        <span class="px-3 py-1.5 rounded-lg border border-hair bg-[rgba(255,255,255,0.02)] text-dim">
          {{ status.n_in || 2 }} IN · {{ nOut }} OUT
        </span>
        <span
          class="px-3 py-1.5 rounded-lg border flex items-center gap-2"
          :class="
            wsState === 'live'
              ? 'border-[color-mix(in_oklab,var(--color-signal)_40%,transparent)] text-signal'
              : wsState === 'down'
                ? 'border-[color-mix(in_oklab,var(--color-hot)_40%,transparent)] text-hot'
                : 'border-hair text-amber'
          "
        >
          <span
            class="w-1.5 h-1.5 rounded-full"
            :class="{
              'bg-signal dot-live': wsState === 'live',
              'bg-hot': wsState === 'down',
              'bg-amber': wsState === 'connecting',
            }"
            :style="wsState === 'live' ? 'color: var(--color-signal)' : ''"
          />
          {{ wsState === 'live' ? 'STREAM LIVE' : wsState === 'down' ? 'OFFLINE' : 'CONNECTING' }}
        </span>
      </div>
    </header>

    <!-- load error -->
    <div
      v-if="loadError"
      class="panel p-8 text-center rise"
      style="--accent: var(--color-hot)"
    >
      <p class="font-display font-bold text-lg text-hot mb-2">Connection lost</p>
      <p class="readout text-[13px] text-dim mb-5">{{ loadError }}</p>
      <button class="chip is-active px-5 py-2.5" style="--accent: var(--color-hot)" @click="() => location.reload()">
        Retry
      </button>
    </div>

    <!-- loading -->
    <div v-else-if="!loaded" class="grid place-items-center py-32">
      <div class="flex flex-col items-center gap-4">
        <div class="loader-ring" />
        <p class="readout text-[12px] tracking-[0.2em] text-faint uppercase">Reading DSP graph…</p>
      </div>
    </div>

    <!-- main grid -->
    <div v-else class="grid lg:grid-cols-12 gap-5">
      <div class="lg:col-span-8">
        <SourceBar :now-playing="nowPlaying" @play="onPlay" @stop="onStop" />
      </div>
      <div class="lg:col-span-4">
        <MasterMeters :meters="meters" :channels="meterChannels" :ws-state="wsState" />
      </div>

      <div class="lg:col-span-12">
        <RoutingPanel v-model="cfg.routing.preset" />
      </div>

      <div
        v-for="(ch, i) in cfg.channels"
        :key="ch._id"
        class="lg:col-span-6"
      >
        <ChannelStrip :channel="ch" :accent="ACCENTS[i % ACCENTS.length]" :fs="fs" :index="i" />
      </div>
    </div>

    <footer class="mt-8 text-center readout text-[10px] tracking-[0.18em] text-faint uppercase">
      Y//SCALE DSP — changes apply live · safe-by-default output
    </footer>

    <Toast :toast="toast" />
  </div>
</template>

<style scoped>
.loader-ring {
  width: 40px;
  height: 40px;
  border-radius: 50%;
  border: 2px solid color-mix(in oklab, var(--color-signal) 18%, transparent);
  border-top-color: var(--color-signal);
  animation: spin 0.8s linear infinite;
  box-shadow: 0 0 20px -4px color-mix(in oklab, var(--color-signal) 60%, transparent);
}
@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
