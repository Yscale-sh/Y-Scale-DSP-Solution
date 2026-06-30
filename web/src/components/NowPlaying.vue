<script setup>
import { ref, computed, watch, onMounted, onBeforeUnmount } from 'vue'

const props = defineProps({
  now: { type: Object, required: true },
  meters: { type: Array, default: () => [] }, // LINEAR peaks per channel
})
const emit = defineEmits(['pause', 'stop', 'seek'])

const reduced = typeof window !== 'undefined' && window.matchMedia
  ? window.matchMedia('(prefers-reduced-motion: reduce)').matches
  : false

// ── state derivations ─────────────────────────────────────────────────────────
const st = computed(() => props.now?.state || 'stopped')
const isPlaying = computed(() => st.value === 'playing')
const isPaused = computed(() => st.value === 'paused')
const isLoading = computed(() => st.value === 'loading')
const isActive = computed(() => isPlaying.value || isPaused.value || isLoading.value)

const title = computed(() => props.now?.title || (isActive.value ? 'Streaming…' : 'Nothing playing'))
const artist = computed(() => props.now?.artist || '')
const album = computed(() => props.now?.album || '')
const art = computed(() => props.now?.art_url || '')
const hasArt = ref(false) // becomes true only once the image actually loads
watch(art, () => { hasArt.value = false })

const duration = computed(() => Number(props.now?.duration) || 0)
const hasDuration = computed(() => duration.value > 1)

const SOURCE_META = {
  'yscale-media': { label: 'YSCALE MEDIA', accent: 'var(--color-signal)' },
  yscale: { label: 'YSCALE MEDIA', accent: 'var(--color-signal)' },
  stream: { label: 'STREAM', accent: 'var(--color-cool)' },
  dlna: { label: 'DLNA', accent: 'var(--color-violet)' },
  generator: { label: 'TEST SIGNAL', accent: 'var(--color-amber)' },
}
const sourceInfo = computed(() => SOURCE_META[props.now?.source] || null)

// ── transport ─────────────────────────────────────────────────────────────────
function togglePlay() {
  if (!isActive.value) return
  emit('pause', !isPlaying.value) // pause when playing, resume when paused
}
function stop() {
  emit('stop')
}
function nudge(sec) {
  if (!hasDuration.value) return
  emit('seek', Math.min(duration.value, Math.max(0, localPos.value + sec)))
}

// ── position interpolation (smooth between ~4 Hz server updates) ───────────────
const localPos = ref(0)
let basePos = 0
let baseT = 0
const seeking = ref(false)
const seekPos = ref(0)

watch(
  () => [props.now?.position, props.now?.state],
  () => {
    basePos = Number(props.now?.position) || 0
    baseT = performance.now()
    if (!seeking.value) localPos.value = basePos
  },
)

const displayPos = computed(() => (seeking.value ? seekPos.value : localPos.value))
const progressPct = computed(() =>
  hasDuration.value ? Math.min(100, Math.max(0, (displayPos.value / duration.value) * 100)) : 0,
)

function fmtTime(s) {
  if (!isFinite(s) || s < 0) s = 0
  const m = Math.floor(s / 60)
  const sec = Math.floor(s % 60)
  return `${m}:${sec.toString().padStart(2, '0')}`
}

// ── seek interaction ──────────────────────────────────────────────────────────
const barEl = ref(null)
function posFromEvent(e) {
  const rect = barEl.value.getBoundingClientRect()
  const x = (e.touches ? e.touches[0].clientX : e.clientX) - rect.left
  return Math.min(1, Math.max(0, x / rect.width)) * duration.value
}
function onSeekDown(e) {
  if (!hasDuration.value) return
  seeking.value = true
  seekPos.value = posFromEvent(e)
  window.addEventListener('pointermove', onSeekMove)
  window.addEventListener('pointerup', onSeekUp)
}
function onSeekMove(e) {
  if (seeking.value) seekPos.value = posFromEvent(e)
}
function onSeekUp() {
  if (seeking.value) {
    emit('seek', seekPos.value)
    localPos.value = seekPos.value
    seeking.value = false
  }
  window.removeEventListener('pointermove', onSeekMove)
  window.removeEventListener('pointerup', onSeekUp)
}

// ── the alive bit: canvas visualizer + breathing glow driven by live meters ───
const canvas = ref(null)
const energy = ref(0) // 0..1 smoothed level → drives the art-frame glow
let raf = 0
let bars = []
const N = 72

function loop(t) {
  raf = requestAnimationFrame(loop)

  // smoothed energy from peak meters
  let lvl = 0
  for (const m of props.meters) lvl = Math.max(lvl, m || 0)
  lvl = Math.min(1, Math.pow(lvl, 0.6)) // perceptual lift
  const target = isPlaying.value ? lvl : 0
  energy.value += (target - energy.value) * (target > energy.value ? 0.4 : 0.06)

  // smooth position between server syncs
  if (!seeking.value && isPlaying.value && hasDuration.value) {
    localPos.value = Math.min(duration.value, basePos + (t - baseT) / 1000)
  }

  drawViz(t)
}

function drawViz(t) {
  const cv = canvas.value
  if (!cv) return
  const ctx = cv.getContext('2d')
  const dpr = Math.min(2, window.devicePixelRatio || 1)
  const w = cv.clientWidth
  const h = cv.clientHeight
  if (cv.width !== w * dpr || cv.height !== h * dpr) {
    cv.width = w * dpr
    cv.height = h * dpr
  }
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
  ctx.clearRect(0, 0, w, h)
  if (bars.length !== N) bars = new Array(N).fill(0)

  const e = energy.value
  const cx = w / 2
  const cy = h / 2
  const base = Math.min(w, h) * 0.30
  const sig = '31, 240, 200' // --color-signal rgb

  // radial reactive bars — a phosphor bloom that dances with the music
  for (let i = 0; i < N; i++) {
    // per-bar smooth motion so it reads as a spectrum, not a single throb
    const lfo = 0.5 + 0.5 * Math.sin(t / 480 + i * 0.7) * Math.sin(t / 1130 + i * 1.9)
    const tgt = e * (0.25 + 0.95 * lfo)
    bars[i] += (tgt - bars[i]) * (tgt > bars[i] ? 0.5 : 0.12)
    const a = (i / N) * Math.PI * 2
    const inner = base * 0.92
    const len = base * (0.12 + bars[i] * 1.05)
    const x1 = cx + Math.cos(a) * inner
    const y1 = cy + Math.sin(a) * inner
    const x2 = cx + Math.cos(a) * (inner + len)
    const y2 = cy + Math.sin(a) * (inner + len)
    ctx.strokeStyle = `rgba(${sig}, ${0.18 + bars[i] * 0.7})`
    ctx.lineWidth = 2.4
    ctx.lineCap = 'round'
    ctx.beginPath()
    ctx.moveTo(x1, y1)
    ctx.lineTo(x2, y2)
    ctx.stroke()
  }

  // glowing core ring
  ctx.beginPath()
  ctx.arc(cx, cy, base * 0.92, 0, Math.PI * 2)
  ctx.strokeStyle = `rgba(${sig}, ${0.35 + e * 0.5})`
  ctx.lineWidth = 1.5
  ctx.stroke()
  const grad = ctx.createRadialGradient(cx, cy, 0, cx, cy, base)
  grad.addColorStop(0, `rgba(${sig}, ${0.10 + e * 0.28})`)
  grad.addColorStop(1, 'rgba(0,0,0,0)')
  ctx.fillStyle = grad
  ctx.beginPath()
  ctx.arc(cx, cy, base, 0, Math.PI * 2)
  ctx.fill()
}

onMounted(() => {
  if (reduced) {
    drawViz(0)
    return
  }
  raf = requestAnimationFrame(loop)
})
onBeforeUnmount(() => {
  cancelAnimationFrame(raf)
  window.removeEventListener('pointermove', onSeekMove)
  window.removeEventListener('pointerup', onSeekUp)
})

// glow strength for the art frame
const glow = computed(() => 0.15 + energy.value * 0.85)
</script>

<template>
  <section class="panel rise overflow-hidden" style="--accent: var(--color-signal); animation-delay: 0ms">
    <div class="grid md:grid-cols-[minmax(0,300px)_1fr] gap-6 md:gap-8 p-5 md:p-7">
      <!-- ART / VISUALIZER — the signature, breathing with the audio -->
      <div class="relative mx-auto w-full max-w-[300px] aspect-square">
        <!-- reactive glow halo -->
        <div
          class="absolute inset-0 rounded-2xl transition-shadow duration-150 will-change-transform"
          :style="{
            boxShadow: `0 0 ${30 + glow * 90}px ${-10 + glow * 10}px color-mix(in oklab, var(--color-signal) ${20 + glow * 55}%, transparent)`,
            transform: `scale(${1 + energy * 0.02})`,
          }"
        />
        <div
          class="absolute inset-0 rounded-2xl overflow-hidden border border-hair bg-[#04070a] grid place-items-center"
        >
          <!-- cover art -->
          <img
            v-show="hasArt"
            :src="art"
            alt=""
            class="absolute inset-0 w-full h-full object-cover"
            @load="hasArt = true"
            @error="hasArt = false"
          />
          <!-- live canvas (under art if any, full bloom otherwise) -->
          <canvas
            ref="canvas"
            class="absolute inset-0 w-full h-full transition-opacity duration-500"
            :class="hasArt ? 'opacity-0' : 'opacity-100'"
          />
          <!-- idle waveform mark when truly nothing is up -->
          <svg
            v-if="!hasArt && !isActive"
            class="relative w-16 h-16 opacity-40"
            viewBox="0 0 24 24" fill="none" stroke="var(--color-signal)" stroke-width="1.5" stroke-linecap="round"
          >
            <path d="M2 12h3l2-7 4 14 3-9 2 4h6" />
          </svg>
          <!-- loading shimmer -->
          <div v-if="isLoading" class="absolute inset-x-0 bottom-0 h-0.5 overflow-hidden">
            <div class="h-full w-1/3 bg-signal animate-[load_1.1s_ease-in-out_infinite]" />
          </div>
        </div>
      </div>

      <!-- META + TRANSPORT -->
      <div class="flex flex-col min-w-0">
        <!-- source + state -->
        <div class="flex items-center gap-2.5 mb-3 flex-wrap">
          <span
            v-if="sourceInfo"
            class="readout text-[10px] tracking-[0.18em] px-2.5 py-1 rounded-full border"
            :style="{
              color: sourceInfo.accent,
              borderColor: `color-mix(in oklab, ${sourceInfo.accent} 40%, transparent)`,
              background: `color-mix(in oklab, ${sourceInfo.accent} 10%, transparent)`,
            }"
          >{{ sourceInfo.label }}</span>
          <span class="flex items-center gap-1.5 readout text-[10px] tracking-[0.16em] text-faint uppercase">
            <span
              class="w-1.5 h-1.5 rounded-full"
              :class="isPlaying ? 'bg-signal dot-live' : isPaused ? 'bg-amber' : 'bg-faint'"
              :style="isPlaying ? 'color:var(--color-signal)' : ''"
            />
            {{ isLoading ? 'Buffering' : isPlaying ? 'Playing' : isPaused ? 'Paused' : 'Idle' }}
          </span>
        </div>

        <!-- title / artist / album -->
        <h2
          class="font-display font-bold tracking-tight text-ink leading-tight text-2xl sm:text-[32px] truncate"
          :title="title"
        >{{ title }}</h2>
        <p v-if="artist" class="text-dim text-base sm:text-lg mt-1 truncate" :title="artist">{{ artist }}</p>
        <p v-if="album" class="readout text-[12px] text-faint mt-0.5 truncate" :title="album">{{ album }}</p>

        <div class="flex-1 min-h-[14px]" />

        <!-- progress (or LIVE) -->
        <div class="mt-4">
          <template v-if="hasDuration">
            <div
              ref="barEl"
              class="group relative h-2.5 rounded-full bg-[#04070a] border border-hair cursor-pointer touch-none"
              @pointerdown="onSeekDown"
            >
              <div
                class="absolute inset-y-0 left-0 rounded-full"
                style="background: linear-gradient(90deg, var(--color-signal-deep), var(--color-signal)); box-shadow: 0 0 12px -2px var(--color-signal)"
                :style="{ width: progressPct + '%' }"
              />
              <div
                class="absolute top-1/2 -translate-y-1/2 -translate-x-1/2 w-3.5 h-3.5 rounded-full bg-white opacity-0 group-hover:opacity-100 transition-opacity"
                style="box-shadow: 0 0 10px var(--color-signal)"
                :style="{ left: progressPct + '%' }"
              />
            </div>
            <div class="flex justify-between readout text-[11px] text-faint mt-1.5 tabular-nums">
              <span>{{ fmtTime(displayPos) }}</span>
              <span>-{{ fmtTime(Math.max(0, duration - displayPos)) }}</span>
            </div>
          </template>
          <template v-else-if="isActive">
            <div class="flex items-center gap-2.5">
              <span class="readout text-[11px] tracking-[0.2em] text-signal">LIVE</span>
              <div class="flex items-end gap-0.5 h-4">
                <span v-for="i in 5" :key="i" class="w-0.5 bg-signal rounded-full eqbar" :style="{ animationDelay: i * 0.12 + 's' }" />
              </div>
            </div>
          </template>
        </div>

        <!-- transport -->
        <div class="flex items-center gap-3 mt-5">
          <button
            class="ctl"
            :disabled="!hasDuration"
            title="Back 15s"
            @click="nudge(-15)"
          >
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M11 17l-5-5 5-5M18 17l-5-5 5-5" />
            </svg>
          </button>

          <button
            class="play-btn"
            :class="{ 'is-idle': !isActive }"
            :disabled="!isActive"
            @click="togglePlay"
          >
            <svg v-if="isPlaying" width="26" height="26" viewBox="0 0 24 24" fill="currentColor"><rect x="6" y="5" width="4" height="14" rx="1" /><rect x="14" y="5" width="4" height="14" rx="1" /></svg>
            <svg v-else width="26" height="26" viewBox="0 0 24 24" fill="currentColor"><path d="M7 4l13 8-13 8z" /></svg>
          </button>

          <button
            class="ctl"
            :disabled="!hasDuration"
            title="Forward 30s"
            @click="nudge(30)"
          >
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M13 17l5-5-5-5M6 17l5-5-5-5" />
            </svg>
          </button>

          <button
            class="ctl ml-1"
            :class="{ 'is-hot': isActive }"
            :disabled="!isActive"
            title="Stop"
            @click="stop"
          >
            <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor"><rect x="5" y="5" width="14" height="14" rx="2" /></svg>
          </button>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.ctl {
  display: grid;
  place-items: center;
  width: 44px;
  height: 44px;
  border-radius: 12px;
  color: var(--color-dim);
  border: 1px solid var(--color-hair);
  background: linear-gradient(180deg, rgba(255, 255, 255, 0.03), rgba(255, 255, 255, 0));
  transition: all 0.16s ease;
  flex: none;
}
.ctl:hover:not(:disabled) {
  color: var(--color-ink);
  border-color: color-mix(in oklab, var(--color-signal) 40%, var(--color-edge));
  transform: translateY(-1px);
}
.ctl:disabled {
  opacity: 0.3;
  cursor: not-allowed;
}
.ctl.is-hot:hover:not(:disabled) {
  color: var(--color-hot);
  border-color: color-mix(in oklab, var(--color-hot) 50%, transparent);
}

.play-btn {
  display: grid;
  place-items: center;
  width: 64px;
  height: 64px;
  border-radius: 50%;
  flex: none;
  color: var(--color-void);
  background: linear-gradient(180deg, color-mix(in oklab, var(--color-signal) 100%, white 12%), var(--color-signal-deep));
  box-shadow: 0 14px 40px -12px color-mix(in oklab, var(--color-signal) 85%, transparent);
  transition: transform 0.14s ease, box-shadow 0.2s ease;
}
.play-btn:hover:not(:disabled) {
  transform: scale(1.05);
}
.play-btn:active:not(:disabled) {
  transform: scale(0.97);
}
.play-btn.is-idle {
  background: var(--color-surface-2);
  color: var(--color-faint);
  box-shadow: none;
  opacity: 0.5;
  cursor: not-allowed;
}

.eqbar {
  height: 35%;
  animation: eq 0.9s ease-in-out infinite;
}
@keyframes eq {
  0%, 100% { height: 25%; }
  50% { height: 100%; }
}
@keyframes load {
  0% { transform: translateX(-120%); }
  100% { transform: translateX(420%); }
}
@media (prefers-reduced-motion: reduce) {
  .eqbar { animation: none; height: 60%; }
}
</style>
