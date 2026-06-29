<script setup>
import { ref, watch, onMounted, onBeforeUnmount } from 'vue'
import { lin2db, clamp } from '../lib/dsp.js'

const props = defineProps({
  meters: { type: Array, default: () => [] }, // LINEAR peaks
  channels: { type: Array, default: () => [] }, // [{ name, accent }]
  wsState: { type: String, default: 'connecting' },
})

const FLOOR = -60
const TICKS = [0, -6, -12, -18, -24, -36, -48, -60]

const disp = ref([]) // smoothed dB for the bar
const peak = ref([]) // held peak dB
const clip = ref([]) // clip flag per channel
let peakHold = [] // ms remaining of hold per channel
let raf = 0
let lastT = 0

function ensureLen(n) {
  for (const arr of [disp, peak]) {
    while (arr.value.length < n) arr.value.push(FLOOR)
    arr.value.length = n
  }
  while (clip.value.length < n) clip.value.push(false)
  clip.value.length = n
  while (peakHold.length < n) peakHold.push(0)
  peakHold.length = n
}

function pct(db) {
  return clamp((db - FLOOR) / (0 - FLOOR), 0, 1) * 100
}

function tick(t) {
  raf = requestAnimationFrame(tick)
  const dt = Math.min(64, t - lastT || 16)
  lastT = t
  const n = Math.max(props.channels.length, props.meters.length)
  ensureLen(n)

  for (let i = 0; i < n; i++) {
    const lin = props.meters[i] ?? 0
    const target = clamp(lin2db(lin), FLOOR, 6)
    // VU ballistics: snappy attack, gentle release.
    const cur = disp.value[i]
    const coeff = target > cur ? 0.55 : 1 - Math.exp(-dt / 220)
    disp.value[i] = cur + (target - cur) * coeff

    clip.value[i] = target >= -0.3

    // Peak hold with decay.
    if (target >= peak.value[i]) {
      peak.value[i] = target
      peakHold[i] = 900
    } else if (peakHold[i] > 0) {
      peakHold[i] -= dt
    } else {
      peak.value[i] = Math.max(FLOOR, peak.value[i] - dt * 0.018)
    }
  }
  // trigger reactivity
  disp.value = disp.value.slice()
  peak.value = peak.value.slice()
}

watch(() => props.channels.length, (n) => ensureLen(n), { immediate: true })

onMounted(() => {
  ensureLen(Math.max(props.channels.length, 2))
  raf = requestAnimationFrame(tick)
})
onBeforeUnmount(() => cancelAnimationFrame(raf))

const stateLabel = { live: 'LIVE', connecting: 'LINK…', down: 'OFFLINE' }
</script>

<template>
  <section class="panel rise p-5 md:p-6" style="--accent: var(--color-signal); animation-delay: 80ms">
    <header class="flex items-center justify-between mb-5">
      <div>
        <p class="eyebrow">Master · Output</p>
        <h2 class="font-display font-bold text-lg tracking-wide text-ink mt-0.5">LIVE METERS</h2>
      </div>
      <div class="flex items-center gap-2">
        <span
          class="w-2 h-2 rounded-full"
          :class="{
            'bg-signal dot-live': wsState === 'live',
            'bg-amber': wsState === 'connecting',
            'bg-hot': wsState === 'down',
          }"
          :style="wsState === 'live' ? 'color:var(--color-signal)' : ''"
        />
        <span class="readout text-[10px] tracking-[0.18em] text-faint">{{ stateLabel[wsState] }}</span>
      </div>
    </header>

    <div class="flex gap-5 md:gap-7 justify-center" style="--mh: 200px">
      <!-- shared dB scale -->
      <div class="relative hidden sm:block" :style="{ height: 'var(--mh)' }">
        <div
          v-for="t in TICKS"
          :key="t"
          class="absolute right-0 readout text-[9px] text-faint -translate-y-1/2 pr-1 tabular-nums"
          :style="{ top: `${100 - pct(t)}%` }"
        >
          {{ t }}
        </div>
      </div>

      <div
        v-for="(ch, i) in channels"
        :key="i"
        class="flex flex-col items-center gap-2 flex-1 max-w-[120px]"
        :style="{ '--accent': ch.accent }"
      >
        <!-- the meter -->
        <div
          class="relative w-full rounded-lg overflow-hidden border border-hair bg-[#04070a]"
          :style="{ height: 'var(--mh)' }"
        >
          <!-- grid -->
          <div
            v-for="t in TICKS"
            :key="t"
            class="absolute left-0 right-0 border-t border-[rgba(255,255,255,0.05)]"
            :style="{ top: `${100 - pct(t)}%` }"
          />
          <!-- ghost full gradient -->
          <div class="absolute inset-0 meter-grad opacity-[0.08]" />
          <!-- active fill -->
          <div
            class="absolute bottom-0 left-0 right-0 meter-grad transition-[height] duration-75 ease-out"
            :style="{ height: `${pct(disp[i] ?? FLOOR)}%` }"
          />
          <!-- peak cap -->
          <div
            class="absolute left-0 right-0 h-[2px] bg-white"
            style="box-shadow: 0 0 8px rgba(255, 255, 255, 0.9)"
            :style="{ bottom: `calc(${pct(peak[i] ?? FLOOR)}% - 1px)` }"
          />
          <!-- clip LED -->
          <div
            class="absolute top-1.5 left-1/2 -translate-x-1/2 w-2 h-2 rounded-full transition-colors duration-150"
            :class="clip[i] ? 'bg-hot' : 'bg-[rgba(255,255,255,0.08)]'"
            :style="clip[i] ? 'box-shadow:0 0 10px var(--color-hot)' : ''"
          />
        </div>

        <!-- readouts -->
        <div class="text-center leading-tight">
          <div
            class="readout text-base font-semibold tabular-nums"
            :style="{ color: (disp[i] ?? FLOOR) > -3 ? 'var(--color-hot)' : 'var(--color-ink)' }"
          >
            {{ (disp[i] ?? FLOOR) <= FLOOR ? '−∞' : (disp[i] ?? 0).toFixed(1) }}
          </div>
          <div class="readout text-[9px] text-faint tracking-[0.12em]">dBFS</div>
          <div
            class="mt-1.5 font-display font-semibold text-[11px] tracking-[0.16em] uppercase truncate max-w-[110px]"
            :style="{ color: ch.accent }"
          >
            {{ ch.name }}
          </div>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.meter-grad {
  background: linear-gradient(
    to top,
    var(--color-meter-lo) 0%,
    var(--color-meter-lo) 55%,
    var(--color-meter-mid) 80%,
    var(--color-meter-hi) 100%
  );
  background-size: 100% var(--mh);
  background-position: bottom;
}
</style>
