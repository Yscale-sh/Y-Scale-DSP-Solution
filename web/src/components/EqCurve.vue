<script setup>
import { ref, computed, onMounted, onBeforeUnmount } from 'vue'
import {
  sampleResponse,
  freqToNorm,
  normToFreq,
  bandUsesGain,
  clamp,
  fmtHz,
  FREQ_MIN,
  FREQ_MAX,
} from '../lib/dsp.js'

const props = defineProps({
  bands: { type: Array, default: () => [] },
  crossover: { type: Object, default: null },
  graphicEq: { type: Array, default: null },
  fs: { type: Number, default: 48000 },
  accent: { type: String, default: 'var(--color-signal)' },
  selectedId: { type: [String, Number], default: null },
  height: { type: Number, default: 220 },
})
const emit = defineEmits(['band-input', 'select'])

const DBR = 24
const PAD = 14
const FREQ_TICKS = [20, 50, 100, 200, 500, 1000, 2000, 5000, 10000, 20000]
const DB_TICKS = [24, 12, 0, -12, -24]
const DB_FAINT = [18, 6, -6, -18]

const wrap = ref(null)
const W = ref(640)
const H = computed(() => props.height)
let ro

onMounted(() => {
  ro = new ResizeObserver((entries) => {
    const w = entries[0]?.contentRect?.width
    if (w) W.value = Math.max(240, Math.round(w))
  })
  if (wrap.value) ro.observe(wrap.value)
})
onBeforeUnmount(() => ro && ro.disconnect())

const plotH = computed(() => H.value - 2 * PAD)
const xOf = (f) => freqToNorm(f) * W.value
const yOf = (db) => PAD + (1 - (clamp(db, -DBR, DBR) + DBR) / (2 * DBR)) * plotH.value
const freqOfX = (px) => normToFreq(clamp(px / W.value, 0, 1))
const dbOfY = (py) => (1 - (clamp(py, PAD, PAD + plotH.value) - PAD) / plotH.value) * 2 * DBR - DBR

const samples = computed(() => {
  const pts = Math.max(160, Math.round(W.value / 2.4))
  return sampleResponse(props.bands, props.fs, {
    points: pts,
    crossover: props.crossover,
    graphicEq: props.graphicEq,
  })
})

const curveD = computed(() => {
  const s = samples.value
  if (!s.length) return ''
  let d = ''
  for (let i = 0; i < s.length; i++) {
    const x = (s[i].norm * W.value).toFixed(2)
    const y = yOf(s[i].db).toFixed(2)
    d += (i === 0 ? 'M' : 'L') + x + ' ' + y + ' '
  }
  return d.trim()
})

const areaD = computed(() => {
  const s = samples.value
  if (!s.length) return ''
  const y0 = yOf(0).toFixed(2)
  let d = `M0 ${y0} `
  for (const pt of s) d += `L${(pt.norm * W.value).toFixed(2)} ${yOf(pt.db).toFixed(2)} `
  d += `L${W.value.toFixed(2)} ${y0} Z`
  return d
})

const handles = computed(() =>
  props.bands.map((b) => ({
    id: b._id,
    kind: b.kind,
    freq: b.freq,
    usesGain: bandUsesGain(b.kind),
    x: xOf(b.freq),
    y: yOf(bandUsesGain(b.kind) ? b.gain_db : 0),
  })),
)

// ── dragging ──────────────────────────────────────────────────────────────
let drag = null
function onDown(e, band) {
  emit('select', band._id)
  const rect = wrap.value.getBoundingClientRect()
  drag = { id: band._id, kind: band.kind, rect }
  window.addEventListener('pointermove', onMove)
  window.addEventListener('pointerup', onUp)
  e.preventDefault()
}
function onMove(e) {
  if (!drag) return
  const px = e.clientX - drag.rect.left
  const py = e.clientY - drag.rect.top
  const payload = { id: drag.id, freq: clamp(freqOfX(px), FREQ_MIN, FREQ_MAX) }
  if (bandUsesGain(drag.kind)) payload.gain_db = clamp(dbOfY(py), -24, 24)
  emit('band-input', payload)
}
function onUp() {
  drag = null
  window.removeEventListener('pointermove', onMove)
  window.removeEventListener('pointerup', onUp)
}
onBeforeUnmount(onUp)

const fid = `glow-${Math.random().toString(36).slice(2, 8)}`
</script>

<template>
  <div ref="wrap" class="relative w-full select-none touch-none" :style="{ height: H + 'px' }">
    <svg :width="W" :height="H" class="block">
      <defs>
        <filter :id="fid" x="-20%" y="-50%" width="140%" height="200%">
          <feGaussianBlur stdDeviation="3.5" result="b" />
          <feMerge>
            <feMergeNode in="b" />
            <feMergeNode in="SourceGraphic" />
          </feMerge>
        </filter>
        <linearGradient :id="fid + '-area'" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" :stop-color="accent" stop-opacity="0.30" />
          <stop offset="50%" :stop-color="accent" stop-opacity="0.05" />
          <stop offset="100%" :stop-color="accent" stop-opacity="0.14" />
        </linearGradient>
      </defs>

      <!-- faint dB lines -->
      <line
        v-for="db in DB_FAINT"
        :key="'f' + db"
        :x1="0"
        :x2="W"
        :y1="yOf(db)"
        :y2="yOf(db)"
        stroke="rgba(255,255,255,0.04)"
        stroke-width="1"
      />
      <!-- dB grid + labels -->
      <g v-for="db in DB_TICKS" :key="'d' + db">
        <line
          :x1="0"
          :x2="W"
          :y1="yOf(db)"
          :y2="yOf(db)"
          :stroke="db === 0 ? 'rgba(255,255,255,0.18)' : 'rgba(255,255,255,0.07)'"
          :stroke-width="db === 0 ? 1.25 : 1"
        />
        <text :x="6" :y="yOf(db) - 3" class="grid-label" fill="rgba(255,255,255,0.32)">
          {{ db > 0 ? '+' + db : db }}
        </text>
      </g>
      <!-- freq grid + labels -->
      <g v-for="f in FREQ_TICKS" :key="'h' + f">
        <line
          :x1="xOf(f)"
          :x2="xOf(f)"
          :y1="PAD"
          :y2="H - PAD"
          stroke="rgba(255,255,255,0.05)"
          stroke-width="1"
        />
        <text :x="xOf(f) + 3" :y="H - 4" class="grid-label" fill="rgba(255,255,255,0.30)">
          {{ fmtHz(f) }}
        </text>
      </g>

      <!-- area + curve -->
      <path :d="areaD" :fill="`url(#${fid}-area)`" stroke="none" />
      <path
        :d="curveD"
        fill="none"
        :stroke="accent"
        stroke-width="2.5"
        stroke-linejoin="round"
        stroke-linecap="round"
        :filter="`url(#${fid})`"
        opacity="0.95"
      />

      <!-- band handles -->
      <g v-for="h in handles" :key="h.id" style="cursor: grab" @pointerdown="onDown($event, props.bands.find((b) => b._id === h.id))">
        <circle :cx="h.x" :cy="h.y" r="16" fill="transparent" />
        <circle
          :cx="h.x"
          :cy="h.y"
          :r="selectedId === h.id ? 8 : 6"
          :fill="selectedId === h.id ? accent : '#0a0e12'"
          :stroke="accent"
          stroke-width="2"
          :style="
            selectedId === h.id
              ? `filter: drop-shadow(0 0 8px ${accent})`
              : ''
          "
        />
        <text
          v-if="selectedId === h.id"
          :x="clamp(h.x, 26, W - 34)"
          :y="clamp(h.y - 14, 18, H - 6)"
          class="handle-label"
          :fill="accent"
        >
          {{ fmtHz(Math.round(h.freq)) }}Hz
        </text>
      </g>
    </svg>
  </div>
</template>

<style scoped>
.grid-label {
  font-family: var(--font-mono);
  font-size: 9px;
  letter-spacing: 0.02em;
}
.handle-label {
  font-family: var(--font-mono);
  font-size: 10px;
  font-weight: 600;
  text-anchor: middle;
}
svg {
  overflow: visible;
}
</style>
